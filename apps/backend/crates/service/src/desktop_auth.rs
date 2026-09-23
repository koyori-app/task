//! Koyori Desktop の認証（Authorization Code + PKCE、loopback）。
//! 規則は apps/backend/docs/personal-access-tokens-authz.md の「Desktop 認証」。

use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use common::cache::redis::RedisConnection;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use entity::device_tokens;

/// 認可コードの寿命。
pub const CODE_TTL_SECS: u64 = 300;
/// Device Token の寿命。期限切れは 401 で、Desktop はブラウザ承認をやり直す。
pub const DEVICE_TOKEN_TTL_DAYS: i64 = 90;
/// `last_used_at` を書き直す間隔。Desktop は 30 秒ごとにポーリングするので、毎回は書かない。
const LAST_USED_WRITE_INTERVAL_SECS: i64 = 300;

const KEY_CODE: &str = "desktop_auth:code:";
const KEY_TOKEN_RL: &str = "desktop_auth:token_rl:";
/// 交換エンドポイントの試行上限（接続元ごと・窓ごと）。
pub const TOKEN_RATE_LIMIT: i64 = 10;
const TOKEN_RATE_WINDOW_SECS: u64 = 60;

/// 認可コードに紐づけて Redis に置く値。
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingCode {
    pub user_id: Uuid,
    pub code_challenge: String,
    pub name: String,
}

/// 認可コードの Redis キー。
pub fn code_key(code: &str) -> String {
    format!("{KEY_CODE}{code}")
}

/// S256 の code_challenge（`BASE64URL(SHA256(code_verifier))`）。
pub fn s256_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

/// 32 バイト乱数の認可コードを発行し、TTL 付きで保存する。
pub async fn issue_code(
    redis: &RedisConnection,
    pending: &PendingCode,
) -> Result<String, anyhow::Error> {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    let code = URL_SAFE_NO_PAD.encode(buf);

    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let _: () = redis::cmd("SET")
        .arg(code_key(&code))
        .arg(serde_json::to_string(pending)?)
        .arg("EX")
        .arg(CODE_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET desktop code: {e}"))?;
    Ok(code)
}

/// 認可コードを GETDEL で一度きり取り出す。無効・期限切れ・再利用は `None`。
pub async fn take_code(
    redis: &RedisConnection,
    code: &str,
) -> Result<Option<PendingCode>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let payload: Option<String> = redis::cmd("GETDEL")
        .arg(code_key(code))
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GETDEL desktop code: {e}"))?;
    payload
        .map(|s| serde_json::from_str(&s).map_err(|e| anyhow::anyhow!("desktop code decode: {e}")))
        .transpose()
}

/// 交換エンドポイントの試行を 1 回数え、窓内の上限以内なら true。
///
/// `client_key` は接続元の識別子（プロキシが付ける X-Forwarded-For 等）。
/// ponytail: ヘッダは偽装できるので回避は可能。code は 256 bit で総当たりは成立せず、
/// これは多重の守りの 1 枚。信頼できるプロキシ設定が入ったらその値に差し替える
pub async fn try_acquire_token_attempt(
    redis: &RedisConnection,
    client_key: &str,
) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let key = format!("{KEY_TOKEN_RL}{client_key}");
    let (count, _): (i64, i64) = redis::pipe()
        .atomic()
        .cmd("INCR")
        .arg(&key)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(TOKEN_RATE_WINDOW_SECS)
        .arg("NX")
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis desktop token rate limit: {e}"))?;
    Ok(count <= TOKEN_RATE_LIMIT)
}

/// Device Token を発行して保存し、(平文, 行) を返す。平文はこの呼び出し元が 1 度だけ返す。
pub async fn create_device_token(
    db: &DatabaseConnection,
    secret: &str,
    user_id: Uuid,
    name: String,
) -> Result<(String, device_tokens::Model), anyhow::Error> {
    let (token, token_hash) = crate::auth::generate_device_token(secret)
        .map_err(|e| anyhow::anyhow!("generate device token: {e}"))?;
    let now = Utc::now();
    let model = device_tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        name: Set(name),
        token_last_four: Set(token[token.len().saturating_sub(4)..].to_string()),
        token_hash: Set(token_hash),
        expires_at: Set((now + Duration::days(DEVICE_TOKEN_TTL_DAYS)).into()),
        last_used_at: Set(None),
        revoked_at: Set(None),
        created_at: Set(now.into()),
    }
    .insert(db)
    .await?;
    Ok((token, model))
}

/// 認証に使われた Device Token の `last_used_at` を、間隔を空けて更新する。
pub async fn record_device_token_use(
    db: &DatabaseConnection,
    token: &device_tokens::Model,
) -> Result<(), sea_orm::DbErr> {
    let now = Utc::now();
    let stale = token.last_used_at.is_none_or(|t| {
        now.signed_duration_since(t).num_seconds() >= LAST_USED_WRITE_INTERVAL_SECS
    });
    if stale {
        let mut active: device_tokens::ActiveModel = token.clone().into();
        active.last_used_at = Set(Some(now.into()));
        active.update(db).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s256_matches_rfc7636_appendix_b() {
        assert_eq!(
            s256_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}

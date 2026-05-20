//! メール認証トークンを Redis に保持する（TTL で有効期限）。

use uuid::Uuid;

use super::redis::RedisConnection;

/// 認証リンクの有効期限（秒）。約 15 分。
pub const TOKEN_TTL_SECS: u64 = 15 * 60;
/// 認証メール再送のクールダウン（秒）。
pub const RESEND_COOLDOWN_SECS: u64 = 60;

const KEY_TOKEN: &str = "email_verify:t:";
const KEY_USER: &str = "email_verify:u:";
const KEY_RESEND: &str = "email_verify:resend:e:";

pub async fn store_token(
    redis: &RedisConnection,
    user_id: Uuid,
    token: &str,
) -> Result<(), anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let user_key = format!("{KEY_USER}{user_id}");

    let old_token: Option<String> = redis::cmd("GET")
        .arg(&user_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GET user->token: {e}"))?;

    if let Some(ref t) = old_token {
        let _: () = redis::cmd("DEL")
            .arg(format!("{KEY_TOKEN}{t}"))
            .query_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis DEL old token: {e}"))?;
    }

    let token_key = format!("{KEY_TOKEN}{token}");
    let _: () = redis::cmd("SET")
        .arg(&token_key)
        .arg(user_id.to_string())
        .arg("EX")
        .arg(TOKEN_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET token: {e}"))?;

    let _: () = redis::cmd("SET")
        .arg(&user_key)
        .arg(token)
        .arg("EX")
        .arg(TOKEN_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET user->token: {e}"))?;

    Ok(())
}

/// GETDEL でトークンを消費し、対応するユーザー ID を返す。無効・期限切れなら `None`。
pub async fn consume_token(
    redis: &RedisConnection,
    token: &str,
) -> Result<Option<Uuid>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let token_key = format!("{KEY_TOKEN}{token}");
    let raw: Option<String> = redis::cmd("GETDEL")
        .arg(&token_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GETDEL token: {e}"))?;

    let Some(s) = raw else {
        return Ok(None);
    };

    let uid = Uuid::parse_str(s.trim())
        .map_err(|e| anyhow::anyhow!("invalid user id in redis: {e}"))?;

    let user_key = format!("{KEY_USER}{uid}");
    let _: () = redis::cmd("DEL")
        .arg(&user_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis DEL user->token: {e}"))?;

    Ok(Some(uid))
}

/// メールアドレス単位で再送クールダウンを取る。取れたら `true`、取れなければレート制限で `false`。
pub async fn try_acquire_resend_slot(redis: &RedisConnection, email: &str) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let key = format!(
        "{KEY_RESEND}{}",
        email.trim().to_ascii_lowercase()
    );

    let set_ok: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(RESEND_COOLDOWN_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET NX resend cooldown: {e}"))?;

    Ok(set_ok.is_some())
}

//! メール認証トークンを Redis に保持する（TTL で有効期限）。
//!
//! `user->token` と `token->user` は Lua で原子的に更新し、再送時の旧トークン無効化と
//! 消費時の逆マッピング削除が同時リクエストでも崩れないようにする。

use std::sync::LazyLock;

use uuid::Uuid;

use super::redis::RedisConnection;

/// 再送時: 旧 token キー削除 → 新 token/user キー SET を一括実行。
static STORE_TOKEN_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local old_token = redis.call('GET', KEYS[1])
        if old_token then
            redis.call('DEL', ARGV[1] .. old_token)
        end
        redis.call('SET', KEYS[2], ARGV[2], 'EX', tonumber(ARGV[4]))
        redis.call('SET', KEYS[1], ARGV[3], 'EX', tonumber(ARGV[4]))
        return 1
        "#,
    )
});

/// 消費時: GETDEL 後、user->token が当該トークンのときだけ user キーを削除。
static CONSUME_TOKEN_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local user_id = redis.call('GETDEL', KEYS[1])
        if not user_id then
            return nil
        end
        local user_key = ARGV[1] .. user_id
        if redis.call('GET', user_key) == ARGV[2] then
            redis.call('DEL', user_key)
        end
        return user_id
        "#,
    )
});

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
    let token_key = format!("{KEY_TOKEN}{token}");
    let ttl = TOKEN_TTL_SECS.to_string();

    let _: i32 = STORE_TOKEN_SCRIPT
        .key(&user_key)
        .key(&token_key)
        .arg(KEY_TOKEN)
        .arg(user_id.to_string())
        .arg(token)
        .arg(&ttl)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis store_token script: {e}"))?;

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
    let raw: Option<String> = CONSUME_TOKEN_SCRIPT
        .key(&token_key)
        .arg(KEY_USER)
        .arg(token)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis consume_token script: {e}"))?;

    let Some(s) = raw else {
        return Ok(None);
    };

    let uid = Uuid::parse_str(s.trim())
        .map_err(|e| anyhow::anyhow!("invalid user id in redis: {e}"))?;

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

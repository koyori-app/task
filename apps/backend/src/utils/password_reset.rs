//! パスワードリセットトークンを Redis に保持する（メール認証と同様の双方向マッピング）。

use std::sync::LazyLock;

use uuid::Uuid;

use super::email::normalize_email;
use super::redis::RedisConnection;

static STORE_TOKEN_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local user_key = KEYS[1]
        local token_key = KEYS[2]
        local old_token = redis.call('GET', user_key)
        if old_token then
            redis.call('DEL', ARGV[1] .. old_token)
        end
        redis.call('SET', token_key, ARGV[2], 'EX', ARGV[4])
        redis.call('SET', user_key, ARGV[3], 'EX', ARGV[4])
        return 1
        "#,
    )
});

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

/// トークン有効期限（秒）。30 分。
pub const TOKEN_TTL_SECS: u64 = 30 * 60;
/// リセットメール送信のレートリミット（秒）。
pub const RATE_LIMIT_SECS: u64 = 60;

const KEY_TOKEN: &str = "pw_reset:t:";
const KEY_USER: &str = "pw_reset:u:";
const KEY_RL: &str = "pw_reset:rl:";

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

    STORE_TOKEN_SCRIPT
        .key(&user_key)
        .key(&token_key)
        .arg(KEY_TOKEN)
        .arg(user_id.to_string())
        .arg(token)
        .arg(&ttl)
        .invoke_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis store_token script: {e}"))?;

    Ok(())
}

/// Returns the user id bound to a reset token without consuming it.
pub async fn lookup_token_user_id(
    redis: &RedisConnection,
    token: &str,
) -> Result<Option<Uuid>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let token_key = format!("{KEY_TOKEN}{token}");
    let raw: Option<String> = redis::cmd("GET")
        .arg(&token_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GET token: {e}"))?;
    let Some(s) = raw else {
        return Ok(None);
    };
    Ok(Some(
        Uuid::parse_str(s.trim()).map_err(|e| anyhow::anyhow!("invalid user id: {e}"))?,
    ))
}

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

pub async fn token_exists(redis: &RedisConnection, token: &str) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let token_key = format!("{KEY_TOKEN}{token}");
    let exists: bool = redis::cmd("EXISTS")
        .arg(&token_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis EXISTS: {e}"))?;

    Ok(exists)
}

pub async fn try_acquire_rate_limit(
    redis: &RedisConnection,
    email: &str,
) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let key = format!("{KEY_RL}{}", normalize_email(email));

    let set_ok: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(RATE_LIMIT_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET NX rate limit: {e}"))?;

    Ok(set_ok.is_some())
}


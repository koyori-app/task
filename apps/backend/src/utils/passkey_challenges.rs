//! WebAuthn 登録・認証セッション状態を Redis に保持（TTL 5 分）。

use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;
use webauthn_rs::prelude::{
    DiscoverableAuthentication, PasskeyAuthentication, PasskeyRegistration,
};

use super::redis::RedisConnection;

pub const CHALLENGE_TTL_SECS: u64 = 5 * 60;
/// 登録 start〜finish を保護する排他ロック TTL（チャレンジ TTL と揃える）
pub const REGISTRATION_LOCK_TTL_SECS: u64 = CHALLENGE_TTL_SECS;

const KEY_REG: &str = "webauthn:reg:";
const KEY_REG_LOCK: &str = "webauthn:reg:lock:";
const KEY_AUTH: &str = "webauthn:auth:";
const KEY_AUTH_DISC: &str = "webauthn:auth:disc:";

/// 登録フロー排他ロックを取得する（`SET key 1 NX EX REGISTRATION_LOCK_TTL_SECS`）。成功時のみ `true`。
pub async fn acquire_registration_lock(
    redis: &RedisConnection,
    user_id: Uuid,
) -> Result<bool, anyhow::Error> {
    let key = format!("{KEY_REG_LOCK}{user_id}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;

    let acquired: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(REGISTRATION_LOCK_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET NX: {e}"))?;

    Ok(acquired.is_some())
}

/// 登録フロー排他ロックを解放する（`DEL`）。存在しなくても成功。
pub async fn release_registration_lock(
    redis: &RedisConnection,
    user_id: Uuid,
) -> Result<(), anyhow::Error> {
    let key = format!("{KEY_REG_LOCK}{user_id}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;

    let _: () = redis::cmd("DEL")
        .arg(key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis DEL: {e}"))?;

    Ok(())
}

/// 登録フロー排他ロックが保持されているか（finish 前の検証用）。
pub async fn registration_lock_held(
    redis: &RedisConnection,
    user_id: Uuid,
) -> Result<bool, anyhow::Error> {
    let key = format!("{KEY_REG_LOCK}{user_id}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;

    let exists: bool = redis::cmd("EXISTS")
        .arg(key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis EXISTS: {e}"))?;

    Ok(exists)
}

pub async fn store_registration(
    redis: &RedisConnection,
    user_id: Uuid,
    state: &PasskeyRegistration,
) -> Result<(), anyhow::Error> {
    store_json(redis, &format!("{KEY_REG}{user_id}"), state).await
}

pub async fn take_registration(
    redis: &RedisConnection,
    user_id: Uuid,
) -> Result<Option<PasskeyRegistration>, anyhow::Error> {
    take_json(redis, &format!("{KEY_REG}{user_id}")).await
}

pub async fn store_authentication(
    redis: &RedisConnection,
    challenge_id: Uuid,
    state: &PasskeyAuthentication,
) -> Result<(), anyhow::Error> {
    store_json(redis, &format!("{KEY_AUTH}{challenge_id}"), state).await
}

pub async fn take_authentication(
    redis: &RedisConnection,
    challenge_id: Uuid,
) -> Result<Option<PasskeyAuthentication>, anyhow::Error> {
    take_json(redis, &format!("{KEY_AUTH}{challenge_id}")).await
}

pub async fn store_discoverable_authentication(
    redis: &RedisConnection,
    challenge_id: Uuid,
    state: &DiscoverableAuthentication,
) -> Result<(), anyhow::Error> {
    store_json(redis, &format!("{KEY_AUTH_DISC}{challenge_id}"), state).await
}

pub async fn take_discoverable_authentication(
    redis: &RedisConnection,
    challenge_id: Uuid,
) -> Result<Option<DiscoverableAuthentication>, anyhow::Error> {
    take_json(redis, &format!("{KEY_AUTH_DISC}{challenge_id}")).await
}

async fn store_json<T: Serialize>(
    redis: &RedisConnection,
    key: &str,
    value: &T,
) -> Result<(), anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;

    let payload = serde_json::to_string(value)?;
    let _: () = redis::cmd("SET")
        .arg(key)
        .arg(payload)
        .arg("EX")
        .arg(CHALLENGE_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET: {e}"))?;

    Ok(())
}

async fn take_json<T: DeserializeOwned>(
    redis: &RedisConnection,
    key: &str,
) -> Result<Option<T>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;

    let payload: Option<String> = redis::cmd("GETDEL")
        .arg(key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GETDEL: {e}"))?;

    payload
        .map(|s| serde_json::from_str(&s).map_err(|e| anyhow::anyhow!("redis json decode: {e}")))
        .transpose()
}

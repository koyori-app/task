//! WebAuthn 登録・認証セッション状態を Redis に保持（TTL 5 分）。

use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;
use webauthn_rs::prelude::{
    DiscoverableAuthentication, PasskeyAuthentication, PasskeyRegistration,
};

use super::redis::RedisConnection;

pub const CHALLENGE_TTL_SECS: u64 = 5 * 60;

const KEY_REG: &str = "webauthn:reg:";
const KEY_AUTH: &str = "webauthn:auth:";
const KEY_AUTH_DISC: &str = "webauthn:auth:disc:";

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

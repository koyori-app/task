//! GitHub App インストールフロー用 CSRF state（Redis）。

use anyhow::Context;
use base64::Engine;
use rand::Rng;
use redis::AsyncCommands;
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};

use super::redis::RedisConnection;

const KEY_PREFIX: &str = "github_oauth_state:";
const TTL_SECS: u64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubOAuthStatePayload {
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
}

pub fn new_state_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    urlencoding::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)).into_owned()
}

pub async fn store_state(
    redis: &RedisConnection,
    state: &str,
    payload: &GithubOAuthStatePayload,
) -> Result<(), anyhow::Error> {
    let key = format!("{KEY_PREFIX}{state}");
    let value = serde_json::to_string(payload).context("serialize oauth state")?;
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;
    conn.set_ex::<_, _, ()>(key, value, TTL_SECS)
        .await
        .map_err(|e| anyhow::anyhow!("redis set oauth state: {e}"))?;
    Ok(())
}

/// 取得と削除を原子的に行う（再利用防止）。
pub async fn consume_state(
    redis: &RedisConnection,
    state: &str,
) -> Result<Option<GithubOAuthStatePayload>, anyhow::Error> {
    let key = format!("{KEY_PREFIX}{state}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;
    let value: Option<String> = conn
        .get_del(key)
        .await
        .map_err(|e| anyhow::anyhow!("redis getdel oauth state: {e}"))?;
    let Some(raw) = value else {
        return Ok(None);
    };
    let payload: GithubOAuthStatePayload =
        serde_json::from_str(&raw).context("deserialize oauth state")?;
    Ok(Some(payload))
}

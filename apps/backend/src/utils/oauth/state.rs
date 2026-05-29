//! OAuth state を Redis に保存・検証する。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::config::OAuthSettings;
use crate::utils::redis::RedisConnection;

/// OAuth state の TTL（秒）。仕様: 10 分。
pub const STATE_TTL_SECS: u64 = 10 * 60;

const KEY_PREFIX: &str = "oauth:state:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthStatePayload {
    pub code_verifier: String,
    pub redirect_after: String,
    /// アカウント連携時のログイン済みユーザー ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_user_id: Option<Uuid>,
    /// GitLab self-hosted のインスタンス URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_url: Option<String>,
}

pub async fn store_state(
    redis: &RedisConnection,
    state: &str,
    payload: &OAuthStatePayload,
) -> Result<(), anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;

    let key = format!("{KEY_PREFIX}{state}");
    let value = serde_json::to_string(payload)?;
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(value)
        .arg("EX")
        .arg(STATE_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET oauth state: {e}"))?;

    Ok(())
}

/// state を検証して取得し、Redis から即削除する（使い捨て）。
pub async fn consume_state(
    redis: &RedisConnection,
    state: &str,
) -> Result<Option<OAuthStatePayload>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;

    let key = format!("{KEY_PREFIX}{state}");
    let value: Option<String> = redis::cmd("GETDEL")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GETDEL oauth state: {e}"))?;

    let Some(raw) = value else {
        return Ok(None);
    };

    let payload: OAuthStatePayload = serde_json::from_str(&raw)?;
    Ok(Some(payload))
}

/// フロントへのリダイレクト URL を組み立てる。
pub fn build_frontend_redirect(
    frontend_base: &str,
    redirect_after: &str,
    _settings: &OAuthSettings,
) -> String {
    let base = frontend_base.trim_end_matches('/');
    let path = if redirect_after.starts_with('/') {
        redirect_after.to_string()
    } else {
        format!("/{redirect_after}")
    };
    format!("{base}{path}")
}

//! OAuth のクライアントの解決（Client ID Metadata Document、CIMD）。
//!
//! `client_id` は https の URL で、認可サーバーがそこからメタデータの文書を取る。
//! サーバーが外の URL へ GET する経路になるので、次で閉じる。
//! - 取得の前に、設定の許可リスト（[`client_is_allowed`]）と URL の形を確かめる
//! - 名前解決は一度だけ行い、[`crate::favicon::resolve_public`]（https・443・公開アドレスだけ）と
//!   [`is_special_use`]（IANA の特殊用途の範囲）を通ったアドレスに接続を固定する
//! - リダイレクトは追わない。200 以外は失敗。本文は [`MAX_DOCUMENT_BYTES`] まで
//! - キャッシュに無い文書を取りに行く回数は、利用者ごとに [`FETCH_RATE_LIMIT`] 回 /
//!   [`FETCH_RATE_WINDOW_SECS`] 秒まで
//!
//! 文書の中の URL（`jwks_uri`・`logo_uri`）は読まず、取りにも行かない。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::cache::redis::RedisConnection;
use common::settings::Settings;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

/// 読む本文の上限（CIMD の推奨の 5 キロバイト）。
pub const MAX_DOCUMENT_BYTES: usize = 5 * 1024;
/// 同意画面に出すクライアント名の上限（文字数）。超えた分は切る。
pub const MAX_CLIENT_NAME_CHARS: usize = 100;
/// 接続と応答全体の上限。
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// キャッシュの寿命（秒）。応答の `max-age` をこの範囲に丸め、無ければ既定を使う。
const CACHE_TTL_MIN_SECS: u64 = 60;
const CACHE_TTL_MAX_SECS: u64 = 86_400;
const CACHE_TTL_DEFAULT_SECS: u64 = 300;
/// キャッシュに無い文書の取得の上限（利用者ごと・窓ごと）。
pub const FETCH_RATE_LIMIT: i64 = 10;
pub const FETCH_RATE_WINDOW_SECS: u64 = 60;

const KEY_CLIENT: &str = "oauth_server:client:";
const KEY_FETCH_RL: &str = "oauth_server:client_fetch_rl:";

/// 検証を通ったクライアントのメタデータ。Redis のキャッシュにもこの形で置く。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientMetadata {
    pub client_id: String,
    pub client_name: String,
    pub client_uri: Option<String>,
    pub redirect_uris: Vec<String>,
    /// 文書の `grant_types` に `refresh_token` が在るか。偽ならリフレッシュトークンを出さない
    pub allow_refresh: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// 許可リストに無い。
    #[error("client is not allowed")]
    NotAllowed,
    /// `client_id` が CIMD の URL の形でない。
    #[error("invalid client_id")]
    InvalidClientId,
    /// 文書を取れない（接続先の拒否・取得の失敗・回数の制限）。
    #[error("client metadata is unavailable")]
    Unavailable,
    /// 文書の中身が受けられない。
    #[error("invalid client metadata document")]
    InvalidDocument,
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

/// `client_id` が設定の許可リストに在るか。完全一致の文字列比較で、外への取得は起きない。
pub fn client_is_allowed(settings: &Settings, client_id: &str) -> bool {
    settings.oauth_allowed_clients().allows(client_id)
}

/// `client_id` の URL の形を確かめる。https、userinfo なし、`/` 以外のパスあり、
/// `.` `..` のセグメントなし、fragment なし、query なし。
pub fn validate_client_id_url(raw: &str) -> Result<Url, ClientError> {
    let url = Url::parse(raw).map_err(|_| ClientError::InvalidClientId)?;
    let has_dot_segment = || {
        // `Url` は dot segment を解いてしまうので、生の文字列で見る
        let after_scheme = raw.split_once("://").map_or(raw, |(_, rest)| rest);
        after_scheme.split(['/', '?', '#']).skip(1).any(|segment| {
            let segment = segment.to_ascii_lowercase().replace("%2e", ".");
            segment == "." || segment == ".."
        })
    };
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path().is_empty()
        || url.path() == "/"
        || url.fragment().is_some()
        || url.query().is_some()
        || has_dot_segment()
    {
        return Err(ClientError::InvalidClientId);
    }
    Ok(url)
}

// IANA の特殊用途のアドレスの登録簿（2026-11-19 に取得した版）の全部と multicast。
// https://www.iana.org/assignments/iana-ipv4-special-registry/
// https://www.iana.org/assignments/iana-ipv6-special-registry/
// 到達できると書かれている項目（AS112、AMT など）も含めて全部拒否する。
// 親の範囲に含まれる細かい項目（192.0.0.0/29、2001::/32 など）は親だけを置く。
// 登録簿が変わったら、ここを突き合わせて直す。
const SPECIAL_USE_V4: [(Ipv4Addr, u32); 18] = [
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 31, 196, 0), 24),
    (Ipv4Addr::new(192, 52, 193, 0), 24),
    (Ipv4Addr::new(192, 88, 99, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(192, 175, 48, 0), 24),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    // multicast
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    // 予約済み。255.255.255.255/32 を含む
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

const SPECIAL_USE_V6: [(Ipv6Addr, u32); 15] = [
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 128),
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1), 128),
    (Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96),
    (Ipv6Addr::new(0x64, 0xff9b, 1, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x100, 0, 0, 0, 0, 0, 0, 0), 64),
    (Ipv6Addr::new(0x100, 0, 0, 1, 0, 0, 0, 0), 64),
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0x2620, 0x4f, 0x8000, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
    (Ipv6Addr::new(0x5f00, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7),
    (Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10),
    // multicast
    (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8),
];

/// 特殊用途のアドレスか。CIMD は、特殊用途のアドレスへ解決する URL の取得を禁じている。
/// IPv4-mapped（`::ffff:0:0/96`）は IPv4 に戻してから判定する。
pub(crate) fn is_special_use(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => {
            let bits = ip.to_bits();
            SPECIAL_USE_V4.iter().any(|(network, prefix)| {
                let mask = u32::MAX << (32 - prefix);
                bits & mask == network.to_bits() & mask
            })
        }
        IpAddr::V6(ip) => {
            let bits = ip.to_bits();
            SPECIAL_USE_V6.iter().any(|(network, prefix)| {
                let mask = u128::MAX << (128 - prefix);
                bits & mask == network.to_bits() & mask
            })
        }
    }
}

/// 名前解決の結果を確かめる。1 つでも特殊用途なら全体を拒否する。
pub(crate) fn ensure_no_special_use(addrs: &[SocketAddr]) -> Result<(), ClientError> {
    if addrs.is_empty() || addrs.iter().any(|addr| is_special_use(addr.ip())) {
        return Err(ClientError::Unavailable);
    }
    Ok(())
}

fn string_list(value: &Value) -> Option<Vec<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}

fn redirect_uri_is_acceptable(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    if url.fragment().is_some() {
        return false;
    }
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => super::redirect_uri_is_loopback(raw),
        _ => false,
    }
}

/// 取得した文書を検証し、使う項目だけを取り出す。`client_id` は取得に使った URL。
pub fn validate_document(client_id: &str, body: &[u8]) -> Result<ClientMetadata, ClientError> {
    let invalid = || ClientError::InvalidDocument;
    let document: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let document = document.as_object().ok_or_else(invalid)?;

    // 文書の `client_id` は、取得した URL と単純な文字列比較で一致すること
    if document.get("client_id").and_then(Value::as_str) != Some(client_id) {
        return Err(invalid());
    }
    // 公開クライアントの文書に共有の秘密は載らない
    if document.contains_key("client_secret") || document.contains_key("client_secret_expires_at") {
        return Err(invalid());
    }
    let client_name = document
        .get("client_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(invalid)?
        .chars()
        .take(MAX_CLIENT_NAME_CHARS)
        .collect::<String>();
    let client_uri = match document.get("client_uri") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_str().ok_or_else(invalid)?.to_string()),
    };

    let redirect_uris = document
        .get("redirect_uris")
        .and_then(string_list)
        .filter(|uris| !uris.is_empty())
        .ok_or_else(invalid)?;
    if !redirect_uris
        .iter()
        .all(|uri| redirect_uri_is_acceptable(uri))
    {
        return Err(invalid());
    }

    // grant と response は、文書の宣言とこのサーバーが扱うものの積を取る。
    // 宣言が無ければ RFC 7591 の既定（authorization_code / code）として読む
    let grant_types = match document.get("grant_types") {
        None => vec!["authorization_code"],
        Some(value) => string_list(value).ok_or_else(invalid)?,
    };
    if !grant_types.contains(&"authorization_code") {
        return Err(invalid());
    }
    let response_types = match document.get("response_types") {
        None => vec!["code"],
        Some(value) => string_list(value).ok_or_else(invalid)?,
    };
    if !response_types.contains(&"code") {
        return Err(invalid());
    }

    // 認証方式は `none`（公開クライアント）だけを受ける。複数形が在ればそれで判定し、
    // どちらも無ければ `none` として読む（RFC 7591 の既定の client_secret_basic は使わない）
    let singular = match document.get("token_endpoint_auth_method") {
        None => None,
        Some(value) => Some(value.as_str().ok_or_else(invalid)?),
    };
    match document.get("token_endpoint_auth_methods_supported") {
        Some(value) => {
            let supported = string_list(value).ok_or_else(invalid)?;
            if !supported.contains(&"none") {
                return Err(invalid());
            }
            if singular.is_some_and(|method| !supported.contains(&method)) {
                return Err(invalid());
            }
        }
        None => {
            if singular.is_some_and(|method| method != "none") {
                return Err(invalid());
            }
        }
    }

    Ok(ClientMetadata {
        client_id: client_id.to_string(),
        client_name,
        client_uri,
        redirect_uris: redirect_uris.into_iter().map(str::to_string).collect(),
        allow_refresh: grant_types.contains(&"refresh_token"),
    })
}

/// `Cache-Control` の `max-age`（秒）。
fn max_age_of(cache_control: &str) -> Option<u64> {
    cache_control.split(',').find_map(|directive| {
        let (name, value) = directive.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("max-age")
            .then(|| value.trim().trim_matches('"').parse().ok())?
    })
}

/// キャッシュの寿命（秒）。`max-age` を上下の限度に丸め、無ければ既定。
pub(crate) fn cache_ttl_secs(max_age: Option<u64>) -> u64 {
    max_age.map_or(CACHE_TTL_DEFAULT_SECS, |secs| {
        secs.clamp(CACHE_TTL_MIN_SECS, CACHE_TTL_MAX_SECS)
    })
}

/// 検証済みの IP に固定して 1 回だけ GET し、本文と `max-age` を返す。
/// リダイレクトは追わず、200 以外と上限を超える本文は失敗にする。
pub(crate) async fn fetch_document_at(
    url: &Url,
    addrs: &[SocketAddr],
) -> Result<(Vec<u8>, Option<u64>), ClientError> {
    let unavailable = |what: &'static str| {
        move |error: reqwest::Error| {
            tracing::debug!(error = %error, "oauth client metadata: {what}");
            ClientError::Unavailable
        }
    };
    let host = url.host_str().ok_or(ClientError::InvalidClientId)?;
    let client = reqwest::Client::builder()
        .user_agent("task-oauth-client-metadata")
        .no_proxy()
        .resolve_to_addrs(host, addrs)
        .redirect(Policy::none())
        .connect_timeout(FETCH_TIMEOUT)
        .timeout(FETCH_TIMEOUT)
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|error| ClientError::Internal(anyhow::anyhow!("http client build: {error}")))?;
    let mut response = client
        .get(url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(unavailable("fetch failed"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(ClientError::Unavailable);
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_DOCUMENT_BYTES as u64)
    {
        return Err(ClientError::Unavailable);
    }
    let max_age = response
        .headers()
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .and_then(max_age_of);
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(unavailable("body read failed"))?
    {
        if body.len() + chunk.len() > MAX_DOCUMENT_BYTES {
            return Err(ClientError::Unavailable);
        }
        body.extend_from_slice(&chunk);
    }
    Ok((body, max_age))
}

/// メタデータのキャッシュの Redis キー。
pub fn client_cache_key(client_id: &str) -> String {
    format!(
        "{KEY_CLIENT}{}",
        URL_SAFE_NO_PAD.encode(Sha256::digest(client_id.as_bytes()))
    )
}

/// 取得回数の制限の Redis キー。
pub fn fetch_rate_limit_key(user_id: Uuid) -> String {
    format!("{KEY_FETCH_RL}{user_id}")
}

async fn cached_client(
    redis: &RedisConnection,
    client_id: &str,
) -> Result<Option<ClientMetadata>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let cached: Option<String> = redis::cmd("GET")
        .arg(client_cache_key(client_id))
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GET oauth client: {e}"))?;
    // 読めない値は無いものとして取り直す
    Ok(cached.and_then(|raw| serde_json::from_str(&raw).ok()))
}

/// 検証済みのメタデータをキャッシュに置く。
pub async fn cache_client(
    redis: &RedisConnection,
    metadata: &ClientMetadata,
    ttl_secs: u64,
) -> Result<(), anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let _: () = redis::cmd("SET")
        .arg(client_cache_key(&metadata.client_id))
        .arg(serde_json::to_string(metadata)?)
        .arg("EX")
        .arg(ttl_secs)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET oauth client: {e}"))?;
    Ok(())
}

/// 取得を 1 回数え、窓内の上限以内なら true。
async fn try_acquire_fetch(redis: &RedisConnection, user_id: Uuid) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let key = fetch_rate_limit_key(user_id);
    let (count, _): (i64, i64) = redis::pipe()
        .atomic()
        .cmd("INCR")
        .arg(&key)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(FETCH_RATE_WINDOW_SECS)
        .arg("NX")
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis oauth client fetch rate limit: {e}"))?;
    Ok(count <= FETCH_RATE_LIMIT)
}

/// `client_id` からクライアントのメタデータを得る。
///
/// 順は、許可リスト → URL の検証 → キャッシュ → 取得回数の制限 → 取得 → 検証 → キャッシュ。
/// 失敗と不正な文書はキャッシュしない。`user_id` は取得回数を数える利用者。
pub async fn resolve_client(
    redis: &RedisConnection,
    settings: &Settings,
    user_id: Uuid,
    client_id: &str,
) -> Result<ClientMetadata, ClientError> {
    if !client_is_allowed(settings, client_id) {
        return Err(ClientError::NotAllowed);
    }
    let url = validate_client_id_url(client_id)?;
    if let Some(cached) = cached_client(redis, client_id).await? {
        return Ok(cached);
    }
    if !try_acquire_fetch(redis, user_id).await? {
        return Err(ClientError::Unavailable);
    }
    let addrs = crate::favicon::resolve_public(&url)
        .await
        .ok_or(ClientError::Unavailable)?;
    ensure_no_special_use(&addrs)?;
    let (body, max_age) = fetch_document_at(&url, &addrs).await?;
    let metadata = validate_document(client_id, &body)?;
    cache_client(redis, &metadata, cache_ttl_secs(max_age)).await?;
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::super::{redirect_uri_matches, test_settings};
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const CLIENT_ID: &str = "https://client.example/oauth/metadata.json";

    #[test]
    fn client_id_url_accepts_an_https_url_with_a_path() {
        for raw in [
            CLIENT_ID,
            "https://claude.ai/oauth/mcp-oauth-client-metadata",
            "https://chatgpt.com/oauth/codex/client.json",
            "https://client.example:8443/c",
        ] {
            assert!(validate_client_id_url(raw).is_ok(), "{raw}");
        }
    }

    #[test]
    fn client_id_url_rejects_other_shapes() {
        for raw in [
            "http://client.example/oauth/metadata.json",
            "https://client.example",
            "https://client.example/",
            "https://client.example/oauth/metadata.json#frag",
            "https://client.example/oauth/metadata.json?x=1",
            "https://user@client.example/oauth/metadata.json",
            "https://user:pass@client.example/oauth/metadata.json",
            "https://client.example/oauth/../metadata.json",
            "https://client.example/oauth/./metadata.json",
            "https://client.example/oauth/%2e%2e/metadata.json",
            "https://client.example/oauth/%2E/metadata.json",
            "https://client.example/oauth/..",
            "client.example/oauth/metadata.json",
            "not a url",
            "",
        ] {
            assert!(
                matches!(
                    validate_client_id_url(raw),
                    Err(ClientError::InvalidClientId)
                ),
                "{raw}"
            );
        }
    }

    #[test]
    fn allowlist_is_an_exact_match() {
        let settings = test_settings("https://app.example.com", CLIENT_ID);
        assert!(client_is_allowed(&settings, CLIENT_ID));
        for other in [
            "https://client.example/oauth/metadata.json/",
            "https://client.example/oauth/metadata.jso",
            "https://client.example/oauth/",
            "https://client.example/oauth/metadata.json2",
            "HTTPS://client.example/oauth/metadata.json",
        ] {
            assert!(!client_is_allowed(&settings, other), "{other}");
        }
        let any = test_settings("https://app.example.com", "*");
        assert!(client_is_allowed(&any, "https://whatever.example/c.json"));
        let none = test_settings("https://app.example.com", "none");
        assert!(!client_is_allowed(&none, CLIENT_ID));
    }

    #[test]
    fn default_allowlist_takes_the_four_known_clients() {
        let settings = test_settings("https://app.example.com", "");
        for id in common::settings::DEFAULT_OAUTH_ALLOWED_CLIENT_IDS {
            assert!(client_is_allowed(&settings, id), "{id}");
        }
        assert!(!client_is_allowed(
            &settings,
            "https://chatgpt.com/oauth/abc123/client.json"
        ));
        assert!(!client_is_allowed(
            &settings,
            "https://chatgpt.com/oauth/client.json/"
        ));
        assert!(!client_is_allowed(&settings, CLIENT_ID));
    }

    fn document(extra: Value) -> Vec<u8> {
        let mut document = serde_json::json!({
            "client_id": CLIENT_ID,
            "client_name": "Example",
            "redirect_uris": ["https://client.example/callback"],
        });
        for (key, value) in extra.as_object().expect("object") {
            if value.is_null() {
                document.as_object_mut().unwrap().remove(key);
            } else {
                document[key] = value.clone();
            }
        }
        document.to_string().into_bytes()
    }

    fn validated(extra: Value) -> Result<ClientMetadata, ClientError> {
        validate_document(CLIENT_ID, &document(extra))
    }

    fn rejected(extra: Value) -> bool {
        matches!(validated(extra), Err(ClientError::InvalidDocument))
    }

    #[test]
    fn minimal_document_is_accepted() {
        let metadata = validated(serde_json::json!({})).expect("valid");
        assert_eq!(
            metadata,
            ClientMetadata {
                client_id: CLIENT_ID.into(),
                client_name: "Example".into(),
                client_uri: None,
                redirect_uris: vec!["https://client.example/callback".into()],
                allow_refresh: false,
            }
        );
    }

    #[test]
    fn document_must_name_the_fetched_url_and_carry_the_required_fields() {
        assert!(rejected(
            serde_json::json!({ "client_id": "https://client.example/oauth/other.json" })
        ));
        assert!(rejected(
            serde_json::json!({ "client_id": format!("{CLIENT_ID}/") })
        ));
        assert!(rejected(serde_json::json!({ "client_id": null })));
        assert!(rejected(serde_json::json!({ "client_name": null })));
        assert!(rejected(serde_json::json!({ "client_name": "  " })));
        assert!(rejected(serde_json::json!({ "client_name": 1 })));
        assert!(rejected(serde_json::json!({ "redirect_uris": null })));
        assert!(rejected(serde_json::json!({ "redirect_uris": [] })));
        assert!(rejected(
            serde_json::json!({ "redirect_uris": "https://client.example/cb" })
        ));
        assert!(matches!(
            validate_document(CLIENT_ID, b"not json"),
            Err(ClientError::InvalidDocument)
        ));
        assert!(matches!(
            validate_document(CLIENT_ID, b"[]"),
            Err(ClientError::InvalidDocument)
        ));
    }

    #[test]
    fn redirect_uris_must_be_https_or_loopback_http_without_a_fragment() {
        for uri in [
            "https://client.example/callback#frag",
            "http://client.example/callback",
            "http://localhost:1@evil.example/callback",
            "custom-scheme://callback",
            "not a url",
        ] {
            assert!(
                rejected(serde_json::json!({ "redirect_uris": [uri] })),
                "{uri}"
            );
        }
        // 1 つでも受けられない要素が在れば全体を拒否する
        assert!(rejected(serde_json::json!({
            "redirect_uris": ["https://client.example/callback", "http://client.example/cb"]
        })));
        let loopback = validated(serde_json::json!({
            "redirect_uris": ["http://localhost/callback", "http://127.0.0.1/callback"]
        }))
        .expect("loopback http");
        assert_eq!(loopback.redirect_uris.len(), 2);
    }

    #[test]
    fn document_with_a_client_secret_is_rejected() {
        assert!(rejected(serde_json::json!({ "client_secret": "s3cret" })));
        assert!(rejected(
            serde_json::json!({ "client_secret_expires_at": 0 })
        ));
    }

    #[test]
    fn client_name_is_cut_at_the_limit() {
        let at_limit = "あ".repeat(MAX_CLIENT_NAME_CHARS);
        let over = format!("{at_limit}い");
        let kept = validated(serde_json::json!({ "client_name": at_limit })).expect("valid");
        assert_eq!(kept.client_name, at_limit);
        let cut = validated(serde_json::json!({ "client_name": over })).expect("valid");
        assert_eq!(cut.client_name, at_limit);
    }

    #[test]
    fn auth_method_must_include_none() {
        // 複数形が在れば複数形で判定する
        assert!(rejected(serde_json::json!({
            "token_endpoint_auth_methods_supported": ["private_key_jwt"]
        })));
        assert!(
            validated(serde_json::json!({
                "token_endpoint_auth_methods_supported": ["none", "private_key_jwt"]
            }))
            .is_ok()
        );
        // 単数形が複数形に含まれていれば、none でなくても受ける
        assert!(
            validated(serde_json::json!({
                "token_endpoint_auth_method": "private_key_jwt",
                "token_endpoint_auth_methods_supported": ["none", "private_key_jwt"]
            }))
            .is_ok()
        );
        assert!(rejected(serde_json::json!({
            "token_endpoint_auth_method": "client_secret_basic",
            "token_endpoint_auth_methods_supported": ["none", "private_key_jwt"]
        })));
        // 複数形が無ければ単数形で判定する
        assert!(rejected(serde_json::json!({
            "token_endpoint_auth_method": "client_secret_basic"
        })));
        assert!(validated(serde_json::json!({ "token_endpoint_auth_method": "none" })).is_ok());
        // どちらも無ければ none として読む
        assert!(validated(serde_json::json!({})).is_ok());
    }

    #[test]
    fn grant_types_are_intersected_with_what_the_server_supports() {
        let only_code =
            validated(serde_json::json!({ "grant_types": ["authorization_code"] })).expect("valid");
        assert!(!only_code.allow_refresh);
        let with_refresh = validated(serde_json::json!({
            "grant_types": ["authorization_code", "refresh_token"]
        }))
        .expect("valid");
        assert!(with_refresh.allow_refresh);
        let with_jwt_bearer = validated(serde_json::json!({
            "grant_types": [
                "authorization_code",
                "refresh_token",
                "urn:ietf:params:oauth:grant-type:jwt-bearer"
            ]
        }))
        .expect("valid");
        assert!(with_jwt_bearer.allow_refresh);
        assert!(rejected(
            serde_json::json!({ "grant_types": ["refresh_token"] })
        ));
        assert!(rejected(serde_json::json!({ "grant_types": [] })));
        assert!(rejected(
            serde_json::json!({ "grant_types": "authorization_code" })
        ));
        // 宣言が無ければ authorization_code だけとして読む
        assert!(
            !validated(serde_json::json!({}))
                .expect("valid")
                .allow_refresh
        );
    }

    #[test]
    fn response_types_must_include_code() {
        assert!(validated(serde_json::json!({ "response_types": ["code"] })).is_ok());
        assert!(rejected(serde_json::json!({ "response_types": ["token"] })));
        assert!(rejected(serde_json::json!({ "response_types": [] })));
    }

    // 2026-11-19 に取得した 4 つの文書。検証の結果を固定する
    const CLAUDE_AI: &str = r#"{"client_id":"https://claude.ai/oauth/mcp-oauth-client-metadata","client_name":"Claude","client_uri":"https://claude.ai","redirect_uris":["https://claude.ai/api/mcp/auth_callback"],"grant_types":["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:jwt-bearer"],"response_types":["code"],"token_endpoint_auth_method":"none"}"#;
    const CLAUDE_CODE: &str = r#"{"client_id":"https://claude.ai/oauth/claude-code-client-metadata","client_name":"Claude Code","client_uri":"https://claude.ai","redirect_uris":["http://localhost/callback","http://127.0.0.1/callback"],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"token_endpoint_auth_method":"none"}"#;
    const CHATGPT: &str = r#"{"client_id":"https://chatgpt.com/oauth/client.json","client_uri":"https://chatgpt.com/","redirect_uris":["https://chatgpt.com/connector_platform_oauth_redirect"],"token_endpoint_auth_method":"private_key_jwt","token_endpoint_auth_methods_supported":["none","private_key_jwt"],"grant_types":["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:jwt-bearer"],"response_types":["code"],"client_name":"ChatGPT","logo_uri":"https://persistent.oaistatic.com/sonic/misc/openai-logo.png","token_endpoint_auth_signing_alg":"RS256","jwks_uri":"https://chatgpt.com/oauth/jwks.json"}"#;
    const CODEX: &str = r#"{"client_id":"https://chatgpt.com/oauth/codex/client.json","client_uri":"https://chatgpt.com/codex","application_type":"native","redirect_uris":["http://127.0.0.1/callback","http://localhost/callback"],"token_endpoint_auth_method":"none","token_endpoint_auth_methods_supported":["none"],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"client_name":"Codex","logo_uri":"https://persistent.oaistatic.com/sonic/misc/openai-logo.png"}"#;

    #[test]
    fn claude_ai_document_is_accepted() {
        let id = "https://claude.ai/oauth/mcp-oauth-client-metadata";
        let metadata = validate_document(id, CLAUDE_AI.as_bytes()).expect("claude.ai");
        assert_eq!(metadata.client_name, "Claude");
        assert!(metadata.allow_refresh);
        assert_eq!(
            metadata.redirect_uris,
            vec!["https://claude.ai/api/mcp/auth_callback"]
        );
    }

    #[test]
    fn claude_code_document_is_accepted_with_loopback_callbacks() {
        let id = "https://claude.ai/oauth/claude-code-client-metadata";
        let metadata = validate_document(id, CLAUDE_CODE.as_bytes()).expect("Claude Code");
        assert!(metadata.allow_refresh);
        for presented in [
            "http://localhost:53124/callback",
            "http://127.0.0.1:53124/callback",
        ] {
            assert!(
                redirect_uri_matches(&metadata.redirect_uris, presented),
                "{presented}"
            );
        }
    }

    #[test]
    fn chatgpt_document_is_accepted_although_it_prefers_private_key_jwt() {
        let id = "https://chatgpt.com/oauth/client.json";
        let metadata = validate_document(id, CHATGPT.as_bytes()).expect("ChatGPT");
        assert_eq!(metadata.client_name, "ChatGPT");
        assert!(metadata.allow_refresh);
        assert_eq!(
            metadata.redirect_uris,
            vec!["https://chatgpt.com/connector_platform_oauth_redirect"]
        );
        // jwks_uri と logo_uri は取り出さない
        let kept = serde_json::to_value(&metadata).expect("json");
        assert!(kept.get("jwks_uri").is_none());
        assert!(kept.get("logo_uri").is_none());
    }

    #[test]
    fn codex_document_is_accepted_with_loopback_callbacks() {
        let id = "https://chatgpt.com/oauth/codex/client.json";
        let metadata = validate_document(id, CODEX.as_bytes()).expect("Codex");
        assert_eq!(metadata.client_name, "Codex");
        assert!(metadata.allow_refresh);
        assert!(redirect_uri_matches(
            &metadata.redirect_uris,
            "http://127.0.0.1:41000/callback"
        ));
        assert!(!redirect_uri_matches(
            &metadata.redirect_uris,
            "http://127.0.0.1:41000/callback/abc"
        ));
    }

    fn special(raw: &str) -> bool {
        is_special_use(raw.parse().expect("ip"))
    }

    #[test]
    fn special_use_ranges_are_rejected() {
        for raw in [
            // IPv4（表の各範囲から 1 つずつ）
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.31.196.1",
            "192.52.193.1",
            "192.88.99.1",
            "192.168.1.1",
            "192.175.48.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            // IPv6
            "::",
            "::1",
            "64:ff9b::1",
            "64:ff9b:1::1",
            "100::1",
            "100:0:0:1::1",
            "2001::1",
            "2001:db8::1",
            "2002::1",
            "2620:4f:8000::1",
            "3fff::1",
            "5f00::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
            // IPv4-mapped は IPv4 に戻して判定する
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
        ] {
            assert!(special(raw), "{raw}");
        }
    }

    #[test]
    fn public_addresses_are_not_special_use() {
        for raw in [
            "1.1.1.1",
            "8.8.8.8",
            "160.79.104.1",
            "2606:4700::1111",
            "2400:cb00::1",
            "::ffff:1.1.1.1",
        ] {
            assert!(!special(raw), "{raw}");
        }
    }

    #[test]
    fn special_use_range_boundaries() {
        assert!(!special("198.17.255.255"));
        assert!(special("198.18.0.0"));
        assert!(special("198.19.255.255"));
        assert!(!special("198.20.0.0"));
        assert!(!special("100.63.255.255"));
        assert!(special("100.127.255.255"));
        assert!(!special("100.128.0.0"));
        assert!(special("2001:1ff:ffff::1"));
        assert!(!special("2001:200::1"));
        assert!(special("3fff:fff::1"));
        assert!(!special("3ffe::1"));
    }

    #[test]
    fn mixed_resolution_is_rejected_as_a_whole() {
        let addr = |raw: &str| SocketAddr::new(raw.parse().expect("ip"), 443);
        assert!(ensure_no_special_use(&[addr("1.1.1.1")]).is_ok());
        assert!(ensure_no_special_use(&[addr("1.1.1.1"), addr("2606:4700::1111")]).is_ok());
        assert!(matches!(
            ensure_no_special_use(&[addr("1.1.1.1"), addr("198.18.0.1")]),
            Err(ClientError::Unavailable)
        ));
        assert!(matches!(
            ensure_no_special_use(&[]),
            Err(ClientError::Unavailable)
        ));
    }

    #[tokio::test]
    async fn resolve_public_rejects_private_and_loopback_targets() {
        for raw in [
            "https://127.0.0.1/oauth/metadata.json",
            "https://10.0.0.1/oauth/metadata.json",
            "https://[::1]/oauth/metadata.json",
            "https://client.example:8443/oauth/metadata.json",
        ] {
            let url = Url::parse(raw).unwrap();
            assert!(
                crate::favicon::resolve_public(&url).await.is_none(),
                "{raw}"
            );
        }
    }

    #[test]
    fn cache_ttl_is_clamped() {
        assert_eq!(cache_ttl_secs(None), 300);
        assert_eq!(cache_ttl_secs(Some(0)), 60);
        assert_eq!(cache_ttl_secs(Some(59)), 60);
        assert_eq!(cache_ttl_secs(Some(60)), 60);
        assert_eq!(cache_ttl_secs(Some(300)), 300);
        assert_eq!(cache_ttl_secs(Some(86_400)), 86_400);
        assert_eq!(cache_ttl_secs(Some(86_401)), 86_400);
    }

    #[test]
    fn max_age_is_read_from_cache_control() {
        assert_eq!(max_age_of("public, max-age=300"), Some(300));
        assert_eq!(max_age_of("Max-Age=59"), Some(59));
        assert_eq!(max_age_of("no-store"), None);
        assert_eq!(max_age_of("max-age=soon"), None);
    }

    /// 1 回だけ応答するサーバーを立て、`client.invalid` をその IP に固定して GET する。
    /// （取得は https 固定だが、固定した接続先とリダイレクト非追従の挙動はスキームに依らない）
    async fn fetch_with_response(response: Vec<u8>) -> Result<(Vec<u8>, Option<u64>), ClientError> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).await.unwrap();
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..count]);
            }
            let _ = stream.write_all(&response).await;
        });
        let url = Url::parse(&format!(
            "http://client.invalid:{}/oauth/metadata.json",
            addr.port()
        ))
        .unwrap();
        fetch_document_at(&url, &[addr]).await
    }

    fn http_response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    #[tokio::test]
    async fn fetch_returns_the_body_and_max_age() {
        let body = br#"{"client_id":"x"}"#;
        let (fetched, max_age) = fetch_with_response(http_response(
            "200 OK",
            "Cache-Control: public, max-age=300\r\n",
            body,
        ))
        .await
        .expect("fetched");
        assert_eq!(fetched, body);
        assert_eq!(max_age, Some(300));

        let (_, max_age) = fetch_with_response(http_response("200 OK", "", body))
            .await
            .expect("fetched");
        assert_eq!(max_age, None);
    }

    #[tokio::test]
    async fn fetch_does_not_follow_redirects_and_fails_on_non_200() {
        for (status, headers) in [
            (
                "302 Found",
                "Location: https://example.com/elsewhere.json\r\n",
            ),
            ("404 Not Found", ""),
            ("500 Internal Server Error", ""),
            ("204 No Content", ""),
        ] {
            let result = fetch_with_response(http_response(status, headers, b"")).await;
            assert!(matches!(result, Err(ClientError::Unavailable)), "{status}");
        }
    }

    #[tokio::test]
    async fn fetch_reads_at_most_the_size_limit() {
        let at_limit = vec![b'a'; MAX_DOCUMENT_BYTES];
        let (fetched, _) = fetch_with_response(http_response("200 OK", "", &at_limit))
            .await
            .expect("at the limit");
        assert_eq!(fetched.len(), MAX_DOCUMENT_BYTES);

        let over = vec![b'a'; MAX_DOCUMENT_BYTES + 1];
        let result = fetch_with_response(http_response("200 OK", "", &over)).await;
        assert!(matches!(result, Err(ClientError::Unavailable)));

        // Content-Length の無い応答（接続を閉じて終える）でも、読みながら打ち切る
        let mut chunked = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        chunked.extend_from_slice(&over);
        let result = fetch_with_response(chunked).await;
        assert!(matches!(result, Err(ClientError::Unavailable)));
    }
}

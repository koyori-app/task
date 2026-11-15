//! リンク先サイトの favicon の取得とキャッシュ（`GET /v1/favicons/{host}`、TASK-362）。
//!
//! 本文の外部リンクの先頭に出す画像を、ブラウザではなくサーバーが取りに行く。
//! 閲覧者の IP も閲覧内容も外部へ出さず、同じ host への取得はキャッシュで閲覧回数から切り離す。
//!
//! サーバーが任意 host へ GET する経路になるため、SSRF を次で閉じる。
//! - https 固定、`/favicon.ico` 固定、GET のみ。
//! - 名前解決は一度だけ行い、[`crate::webhooks::validate_addresses`]（Webhook と同じ規則）で
//!   private / link-local / メタデータ / loopback を拒否し、接続はその IP に固定する。
//! - リダイレクトは自分で追い、各ホップで同じ検査をする（3 回まで）。接続先のポートは常に 443
//!   （リダイレクト先で任意のポートを指せると、応答時間から公開 IP のポートの開閉を探れる）。
//! - 1 ホップ 5 秒、全ホップ合わせて 8 秒、64 KiB、同時 16 件の上限。取得した本文は先頭バイトで
//!   画像と確かめたものだけ返す。SVG は返さない（自オリジンから配信する画像に script を含み得る
//!   形式を入れない）。
//!
//! 認証済みの利用者なら誰でも任意の host を要求できるので、Redis を埋められないようにも抑える。
//! - キャッシュに残す画像は 64 KiB まで。超える応答は取れなかった扱い（汎用アイコン）にする。
//! - キャッシュに無い host を外へ取りに行く回数は、利用者ごとに [`FETCH_RATE_LIMIT`] 回 /
//!   [`FETCH_RATE_WINDOW_SECS`] 秒まで。超えた分は取りに行かず、保存もしない汎用アイコンを返す。
//!   キャッシュ済みの host は数えない。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bytes::Bytes;
use common::cache::redis::RedisConnection;
use reqwest::redirect::Policy;
use url::Url;
use uuid::Uuid;

/// キャッシュに残す画像の上限。実在の favicon の大半は数 KB〜数十 KB で、これを超える物は
/// 取れなかった扱いにする（Redis に 1 host あたり残す量を抑えるため）。
pub const FAVICON_MAX_BYTES: usize = 64 * 1024;
/// 1 ホップ（名前解決、または 1 回の GET）の上限。
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// 全ホップ合わせた上限。1 ホップずつの上限だけだと、リダイレクトのたびに掛け直されて
/// 同時取得の枠を数十秒占有できてしまう。
const TOTAL_FETCH_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_REDIRECTS: usize = 3;
const CACHE_TTL_HIT_SECS: u64 = 7 * 24 * 60 * 60;
const CACHE_TTL_MISS_SECS: u64 = 60 * 60;
const MAX_IN_FLIGHT_FETCHES: usize = 16;
pub const CACHE_KEY_PREFIX: &str = "favicon:v1:";
pub const RATE_LIMIT_KEY_PREFIX: &str = "favicon:rl:";
/// 利用者 1 人あたり、キャッシュに無い host を外へ取りに行ける回数と窓。
/// 初めて開いた長い文書の外部リンク（30 host 程度）が通り、ワイルドカード DNS の host を
/// 連番で叩く使い方は 1 時間に 180 件ほどで止まる値にする。超えた分は窓が明けるまで汎用アイコンで、
/// 画面を開き直せば（キャッシュが育つので）残りも取れる。
pub const FETCH_RATE_LIMIT: i64 = 30;
pub const FETCH_RATE_WINDOW_SECS: i64 = 600;

/// 取れなかった時に返す汎用アイコン（自前の静的 SVG。script も外部参照も含めない）。
pub const FALLBACK_MIME: &str = "image/svg+xml";
pub const FALLBACK_SVG: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" "##,
    r##"fill="none" stroke="#8b949e" stroke-width="1.2" stroke-linecap="round">"##,
    r##"<circle cx="8" cy="8" r="6.4"/><ellipse cx="8" cy="8" rx="2.7" ry="6.4"/>"##,
    r##"<path d="M1.6 8h12.8"/></svg>"##
);

/// キャッシュと応答で扱う画像形式。キャッシュ値の先頭 1 byte は `添字 + 1`（0 は「取れなかった」）。
const MIMES: [&str; 6] = [
    "image/x-icon",
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    // 予備（添字の追加で既存キャッシュを壊さないための空き）
    "",
];

pub enum Favicon {
    Image {
        mime: &'static str,
        bytes: Bytes,
    },
    /// 汎用アイコンを返す。`cacheable` が false の時は応答もキャッシュさせない（一時的な失敗）。
    Fallback {
        cacheable: bool,
    },
}

/// host を検証して小文字にして返す。frontend の `externalLinkHost` と同じ規則
/// （ドット必須、ラベル 63 文字以下、全長 253 文字以下、英小文字・数字・ハイフン）。
pub fn parse_host(raw: &str) -> Option<String> {
    let host = raw.to_ascii_lowercase();
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    let valid = labels.iter().all(|label| {
        let bytes = label.as_bytes();
        !bytes.is_empty()
            && bytes.len() <= 63
            && bytes[0] != b'-'
            && bytes[bytes.len() - 1] != b'-'
            && bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    });
    valid.then_some(host)
}

/// 先頭バイトから画像形式を判定し Content-Type を返す。SVG は返さない。
pub fn detect_icon_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some(MIMES[0]);
    }
    crate::avatar::detect_image_format(bytes).map(|(_, mime)| mime)
}

fn encode_cache(favicon: &Favicon) -> Option<Vec<u8>> {
    match favicon {
        Favicon::Image { mime, bytes } => {
            let index = MIMES.iter().position(|m| m == mime)?;
            let mut value = Vec::with_capacity(bytes.len() + 1);
            value.push(u8::try_from(index + 1).ok()?);
            value.extend_from_slice(bytes);
            Some(value)
        }
        Favicon::Fallback { cacheable: true } => Some(vec![0]),
        Favicon::Fallback { cacheable: false } => None,
    }
}

fn decode_cache(value: &[u8]) -> Option<Favicon> {
    let (tag, rest) = value.split_first()?;
    if *tag == 0 {
        return Some(Favicon::Fallback { cacheable: true });
    }
    let mime = MIMES.get(usize::from(*tag) - 1).filter(|m| !m.is_empty())?;
    Some(Favicon::Image {
        mime,
        bytes: Bytes::copy_from_slice(rest),
    })
}

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

struct InFlightGuard;

impl InFlightGuard {
    fn acquire() -> Option<Self> {
        if IN_FLIGHT.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT_FETCHES {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self)
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 利用者ごとの外部取得の回数を 1 つ数え、上限内なら true を返す（固定窓）。
async fn try_acquire_fetch_slot(
    conn: &mut impl redis::aio::ConnectionLike,
    user_id: Uuid,
) -> Result<bool, anyhow::Error> {
    let key = format!("{RATE_LIMIT_KEY_PREFIX}{user_id}");
    let (count, _): (i64, i64) = redis::pipe()
        .atomic()
        .cmd("INCR")
        .arg(&key)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(FETCH_RATE_WINDOW_SECS)
        .arg("NX")
        .query_async(conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis favicon rate limit: {e}"))?;
    Ok(count <= FETCH_RATE_LIMIT)
}

/// host の favicon をキャッシュ経由で返す。`host` は [`parse_host`] を通した値。
/// `user_id` は、キャッシュに無い host を取りに行く回数を数える相手。
pub async fn favicon_for_host(
    redis: &RedisConnection,
    user_id: Uuid,
    host: &str,
) -> Result<Favicon, anyhow::Error> {
    let key = format!("{CACHE_KEY_PREFIX}{host}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let cached: Option<Vec<u8>> = redis::cmd("GET")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GET favicon: {e}"))?;
    if let Some(favicon) = cached.as_deref().and_then(decode_cache) {
        return Ok(favicon);
    }

    let Some(_guard) = InFlightGuard::acquire() else {
        return Ok(Favicon::Fallback { cacheable: false });
    };
    if !try_acquire_fetch_slot(&mut conn, user_id).await? {
        return Ok(Favicon::Fallback { cacheable: false });
    }
    let favicon = match fetch_from_host(host).await {
        Some((mime, bytes)) => Favicon::Image { mime, bytes },
        None => Favicon::Fallback { cacheable: true },
    };
    let ttl = match favicon {
        Favicon::Image { .. } => CACHE_TTL_HIT_SECS,
        Favicon::Fallback { .. } => CACHE_TTL_MISS_SECS,
    };
    if let Some(value) = encode_cache(&favicon) {
        let _: () = redis::cmd("SET")
            .arg(&key)
            .arg(value)
            .arg("EX")
            .arg(ttl)
            .query_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis SET favicon: {e}"))?;
    }
    Ok(favicon)
}

/// `https://{host}/favicon.ico` を、リダイレクトを各ホップで検査しながら取る。
/// 失敗の理由は呼び出し側に要らない（汎用アイコンになるだけ）ので None にまとめる。
async fn fetch_from_host(host: &str) -> Option<(&'static str, Bytes)> {
    let url = Url::parse(&format!("https://{host}/favicon.ico")).ok()?;
    fetch_chain(url, TOTAL_FETCH_TIMEOUT, |url| async move {
        match resolve_public(&url).await {
            Some(addrs) => fetch_icon_at(&url, &addrs).await,
            None => Hop::Miss,
        }
    })
    .await
}

/// `step`（1 ホップ分の取得）をリダイレクトの数だけ繰り返す。全ホップ合わせて `total` を超えたら
/// 取れなかったものとして打ち切る（同時取得の枠を長く占有させない）。
async fn fetch_chain<F, Fut>(
    first: Url,
    total: Duration,
    mut step: F,
) -> Option<(&'static str, Bytes)>
where
    F: FnMut(Url) -> Fut,
    Fut: std::future::Future<Output = Hop>,
{
    let chain = async {
        let mut url = first;
        for _ in 0..=MAX_REDIRECTS {
            match step(url).await {
                Hop::Icon(mime, bytes) => return Some((mime, bytes)),
                Hop::Redirect(next) => url = next,
                Hop::Miss => return None,
            }
        }
        None
    };
    match tokio::time::timeout(total, chain).await {
        Ok(found) => found,
        Err(_) => {
            tracing::debug!("favicon fetch exceeded the total timeout");
            None
        }
    }
}

/// 一度だけ名前解決し、公開アドレスだけなら接続先を返す。https 以外・443 以外のポート・
/// private 宛ては None（名前解決の前に判定する）。
async fn resolve_public(url: &Url) -> Option<Vec<SocketAddr>> {
    if url.scheme() != "https" {
        return None;
    }
    let port = url.port_or_known_default().filter(|port| *port == 443)?;
    let addrs: Vec<SocketAddr> = match url.host()? {
        url::Host::Ipv4(ip) => vec![SocketAddr::new(ip.into(), port)],
        url::Host::Ipv6(ip) => vec![SocketAddr::new(ip.into(), port)],
        url::Host::Domain(name) => {
            tokio::time::timeout(FETCH_TIMEOUT, tokio::net::lookup_host((name, port)))
                .await
                .ok()?
                .ok()?
                .collect()
        }
    };
    // loopback は常に拒否する（開発用の例外は作らない）
    crate::webhooks::validate_addresses(false, &addrs).ok()?;
    Some(addrs)
}

pub(crate) enum Hop {
    Icon(&'static str, Bytes),
    Redirect(Url),
    Miss,
}

/// 3xx の `Location` を現在の URL からの相対として解く。
pub(crate) fn next_hop(
    current: &Url,
    status: reqwest::StatusCode,
    location: Option<&str>,
) -> Option<Url> {
    if !status.is_redirection() {
        return None;
    }
    current.join(location?).ok()
}

/// 検証済みの IP に固定して 1 回だけ GET する。リダイレクトは追わず [`Hop::Redirect`] で返す。
pub(crate) async fn fetch_icon_at(url: &Url, addrs: &[SocketAddr]) -> Hop {
    let Some(host) = url.host_str() else {
        return Hop::Miss;
    };
    let client = match reqwest::Client::builder()
        .user_agent("task-favicon-proxy")
        .no_proxy()
        .resolve_to_addrs(host, addrs)
        .redirect(Policy::none())
        .connect_timeout(FETCH_TIMEOUT)
        .timeout(FETCH_TIMEOUT)
        .pool_max_idle_per_host(0)
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            tracing::debug!(error = %e, "favicon client build failed");
            return Hop::Miss;
        }
    };
    let mut resp = match client.get(url.clone()).send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::debug!(error = %e, "favicon fetch failed");
            return Hop::Miss;
        }
    };
    let status = resp.status();
    if status.is_redirection() {
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok());
        return next_hop(url, status, location).map_or(Hop::Miss, Hop::Redirect);
    }
    if status != reqwest::StatusCode::OK {
        return Hop::Miss;
    }
    if resp
        .content_length()
        .is_some_and(|len| len > FAVICON_MAX_BYTES as u64)
    {
        return Hop::Miss;
    }
    let mut body = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > FAVICON_MAX_BYTES {
                    return Hop::Miss;
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => {
                tracing::debug!(error = %e, "favicon body read failed");
                return Hop::Miss;
            }
        }
    }
    match detect_icon_format(&body) {
        Some(mime) => Hop::Icon(mime, Bytes::from(body)),
        None => Hop::Miss,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\0";

    #[test]
    fn parse_host_accepts_valid_and_lowercases() {
        for (raw, want) in [
            ("example.com", "example.com"),
            ("Example.COM", "example.com"),
            ("sub.example.co.jp", "sub.example.co.jp"),
            ("xn--wgv71a.jp", "xn--wgv71a.jp"),
            ("10.0.0.1", "10.0.0.1"),
        ] {
            assert_eq!(parse_host(raw).as_deref(), Some(want), "{raw}");
        }
    }

    #[test]
    fn parse_host_rejects_invalid() {
        let label64 = format!("{}.com", "a".repeat(64));
        let long254 = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "a".repeat(63),
            "a".repeat(63),
            "a".repeat(62)
        );
        assert_eq!(long254.len(), 254);
        for raw in [
            "localhost",
            "Example",
            "-a.com",
            "a-.com",
            "a..com",
            "",
            "exa mple.com",
            "example.com/",
            "example.com:443",
            label64.as_str(),
            long254.as_str(),
        ] {
            assert_eq!(parse_host(raw), None, "{raw}");
        }
    }

    #[test]
    fn parse_host_length_boundaries() {
        let label63 = format!("{}.com", "a".repeat(63));
        assert!(parse_host(&label63).is_some());
        let long253 = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "a".repeat(63),
            "a".repeat(63),
            "a".repeat(61)
        );
        assert_eq!(long253.len(), 253);
        assert!(parse_host(&long253).is_some());
    }

    #[test]
    fn detect_icon_format_by_magic_bytes() {
        assert_eq!(
            detect_icon_format(&[0, 0, 1, 0, 1, 0]),
            Some("image/x-icon")
        );
        assert_eq!(detect_icon_format(PNG), Some("image/png"));
        assert_eq!(detect_icon_format(b"GIF89a.."), Some("image/gif"));
        assert_eq!(
            detect_icon_format(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(
            detect_icon_format(b"RIFF\0\0\0\0WEBPVP8 "),
            Some("image/webp")
        );
        // SVG・HTML・空は返さない
        assert_eq!(
            detect_icon_format(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            None
        );
        assert_eq!(detect_icon_format(b"<!doctype html><html>"), None);
        assert_eq!(detect_icon_format(b""), None);
    }

    #[test]
    fn cache_value_round_trips() {
        for mime in &MIMES[..5] {
            let favicon = Favicon::Image {
                mime,
                bytes: Bytes::from_static(b"payload"),
            };
            let value = encode_cache(&favicon).expect("encode");
            match decode_cache(&value).expect("decode") {
                Favicon::Image { mime: m, bytes } => {
                    assert_eq!(m, *mime);
                    assert_eq!(&bytes[..], b"payload");
                }
                Favicon::Fallback { .. } => panic!("image expected"),
            }
        }
        let value = encode_cache(&Favicon::Fallback { cacheable: true }).expect("encode");
        assert!(matches!(
            decode_cache(&value),
            Some(Favicon::Fallback { cacheable: true })
        ));
        // 一時的な失敗は保存しない。壊れた値は読み捨てる
        assert!(encode_cache(&Favicon::Fallback { cacheable: false }).is_none());
        assert!(decode_cache(&[]).is_none());
        assert!(decode_cache(&[99, 1, 2]).is_none());
        // 予備の添字（空 mime）は画像として復号しない
        assert!(decode_cache(&[6, 1]).is_none());
    }

    #[test]
    fn next_hop_resolves_relative_and_absolute_locations() {
        let current = Url::parse("https://example.com/favicon.ico").unwrap();
        let found = reqwest::StatusCode::FOUND;
        assert_eq!(
            next_hop(&current, found, Some("/icons/f.ico"))
                .unwrap()
                .as_str(),
            "https://example.com/icons/f.ico"
        );
        assert_eq!(
            next_hop(&current, found, Some("https://cdn.example.net/f.ico"))
                .unwrap()
                .as_str(),
            "https://cdn.example.net/f.ico"
        );
        assert!(next_hop(&current, found, None).is_none());
        assert!(next_hop(&current, reqwest::StatusCode::OK, Some("/x")).is_none());
    }

    #[tokio::test]
    async fn resolve_public_rejects_non_https_and_private_literals() {
        for raw in [
            "http://example.com/",
            "https://127.0.0.1/",
            "https://10.0.0.1/",
            "https://169.254.169.254/",
            "https://[::1]/",
            // 443 以外のポートは名前解決の前に拒否する（リダイレクト先でポートを探らせない）
            "https://example.com:8443/favicon.ico",
            "https://example.com:80/",
            "https://8.8.8.8:8443/",
        ] {
            let url = Url::parse(raw).unwrap();
            assert!(resolve_public(&url).await.is_none(), "{raw}");
        }
    }

    /// 1 回だけ応答するサーバーを立て、`favicon.invalid` をその IP に固定して GET する。
    /// （取得は https 固定だが、固定した接続先とリダイレクト非追従の挙動はスキームに依らない）
    async fn fetch_with_response(response: Vec<u8>) -> Hop {
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
            "http://favicon.invalid:{}/favicon.ico",
            addr.port()
        ))
        .unwrap();
        fetch_icon_at(&url, &[addr]).await
    }

    fn http_200(content_type: &str, body: &[u8], content_length: Option<usize>) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            content_length.unwrap_or(body.len())
        )
        .into_bytes();
        head.extend_from_slice(body);
        head
    }

    fn body_of(len: usize) -> Vec<u8> {
        let mut bytes = PNG.to_vec();
        bytes.resize(len, 0);
        bytes
    }

    #[tokio::test]
    async fn fetch_icon_at_accepts_png() {
        match fetch_with_response(http_200("image/png", PNG, None)).await {
            Hop::Icon(mime, bytes) => {
                assert_eq!(mime, "image/png");
                assert_eq!(&bytes[..], PNG);
            }
            _ => panic!("icon expected"),
        }
    }

    #[tokio::test]
    async fn fetch_icon_at_rejects_non_image_body() {
        let hop = fetch_with_response(http_200("text/html", b"<!doctype html><html>", None)).await;
        assert!(matches!(hop, Hop::Miss));
        // Content-Type を image と偽っても本文の先頭バイトで判定する
        let hop = fetch_with_response(http_200("image/png", b"<svg xmlns=\"x\"/>", None)).await;
        assert!(matches!(hop, Hop::Miss));
    }

    #[tokio::test]
    async fn fetch_icon_at_rejects_declared_oversize_before_reading_body() {
        // 宣言だけ上限超で本文は送らない: 本文を読みに行けば timeout まで待つことになる
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FAVICON_MAX_BYTES + 1
        )
        .into_bytes();
        let started = std::time::Instant::now();
        assert!(matches!(fetch_with_response(head).await, Hop::Miss));
        assert!(started.elapsed() < FETCH_TIMEOUT);
    }

    #[tokio::test]
    async fn fetch_icon_at_size_limit_boundary() {
        let at_limit = body_of(FAVICON_MAX_BYTES);
        assert!(matches!(
            fetch_with_response(http_200("image/png", &at_limit, None)).await,
            Hop::Icon(..)
        ));
        let over = body_of(FAVICON_MAX_BYTES + 1);
        assert!(matches!(
            fetch_with_response(http_200("image/png", &over, None)).await,
            Hop::Miss
        ));
    }

    #[tokio::test]
    async fn fetch_icon_at_returns_redirect_without_following() {
        let response = b"HTTP/1.1 302 Found\r\nLocation: /icons/f.ico\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
        match fetch_with_response(response).await {
            Hop::Redirect(next) => assert_eq!(next.path(), "/icons/f.ico"),
            _ => panic!("redirect expected"),
        }
    }

    #[tokio::test]
    async fn fetch_icon_at_misses_on_non_200() {
        let response =
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
        assert!(matches!(fetch_with_response(response).await, Hop::Miss));
    }

    #[test]
    fn resolve_public_accepts_the_default_port_for_ip_literals() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        for raw in ["https://8.8.8.8/", "https://8.8.8.8:443/favicon.ico"] {
            let url = Url::parse(raw).unwrap();
            let addrs = rt.block_on(resolve_public(&url)).expect(raw);
            assert_eq!(addrs, vec!["8.8.8.8:443".parse::<SocketAddr>().unwrap()]);
        }
    }

    fn redirect_to(path: &str) -> Hop {
        Hop::Redirect(Url::parse(&format!("https://example.com{path}")).unwrap())
    }

    async fn chain_with_hop_delay(total: Duration, hop_delay: Duration) -> Option<&'static str> {
        let first = Url::parse("https://example.com/favicon.ico").unwrap();
        let mut hops = 0;
        fetch_chain(first, total, |_| {
            hops += 1;
            let hop = hops;
            async move {
                tokio::time::sleep(hop_delay).await;
                if hop <= MAX_REDIRECTS {
                    redirect_to("/next")
                } else {
                    Hop::Icon("image/png", Bytes::from_static(PNG))
                }
            }
        })
        .await
        .map(|(mime, _)| mime)
    }

    #[tokio::test]
    async fn fetch_chain_gives_up_when_hops_add_up_past_the_total_timeout() {
        // 各ホップは 1 ホップの上限（FETCH_TIMEOUT）に収まるが、4 ホップ合わせると total を超える
        let hop = Duration::from_millis(60);
        assert!(hop < FETCH_TIMEOUT);
        assert_eq!(
            chain_with_hop_delay(Duration::from_millis(150), hop).await,
            None
        );
        // 対照: total に収まるなら最後のホップの画像が取れる
        assert_eq!(
            chain_with_hop_delay(Duration::from_secs(5), hop).await,
            Some("image/png")
        );
    }

    #[test]
    fn in_flight_guard_caps_concurrent_fetches_and_releases_on_drop() {
        let guards: Vec<_> = (0..MAX_IN_FLIGHT_FETCHES)
            .map(|_| InFlightGuard::acquire().expect("under the cap"))
            .collect();
        assert!(InFlightGuard::acquire().is_none(), "cap + 1 is refused");
        // 拒否した呼び出しは枠を持ち出さない（戻した後も cap のまま）
        assert!(InFlightGuard::acquire().is_none());
        drop(guards);
        let again = InFlightGuard::acquire();
        assert!(again.is_some(), "slots come back after drop");
    }
}

//! リクエストごとの Sentry スコープ（`NewSentryLayer` が作る Hub）を整える。

use axum::{
    body::Body,
    extract::{MatchedPath, Request},
    http::header,
    middleware::Next,
    response::Response,
};
use sea_orm::prelude::Uuid;

/// Sentry へ渡すリクエストヘッダー。`Cookie` や独自のトークンヘッダーを新設しても
/// 漏れないよう、除外ではなく許可で絞る。`Referer` はトークン付きの URL を含み得るので入れない。
const FORWARDED_HEADERS: [header::HeaderName; 6] = [
    header::ACCEPT,
    header::CONTENT_LENGTH,
    header::CONTENT_TYPE,
    header::HOST,
    header::ORIGIN,
    header::USER_AGENT,
];

/// ルートに一致しなかったリクエストのパスの代わりに使う値（生のパスは資格を含み得る）
const UNMATCHED_PATH: &str = "unmatched";

/// 認証済みユーザーをスコープに載せる。載せるのは ID だけで、メールアドレス等は載せない。
pub fn set_user(user_id: Uuid) {
    sentry::configure_scope(|scope| {
        scope.set_user(Some(sentry::User {
            id: Some(user_id.to_string()),
            ..Default::default()
        }));
    });
}

/// アクセスを確かめたテナントをスコープのタグに載せる。
pub fn set_tenant(tenant_id: Uuid) {
    sentry::configure_scope(|scope| scope.set_tag("tenant_id", tenant_id));
}

/// `SentryHttpLayer` が載せたリクエスト情報を、資格を含まないものに置き換える。
///
/// `SentryHttpLayer` は URL のクエリ（`?token=` や OAuth の `code`）とパス
/// （`/v1/drive/share/{token}`）、機微と判定しないヘッダーをそのまま送る。トランザクションは
/// `before_send` を通らないので、`SentryHttpLayer` の内側のここで上書きする。
pub async fn scrub_request(req: Request<Body>, next: Next) -> Response {
    let matched = req
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned());
    let request = scrubbed_request(&req, matched.as_deref());
    let method = req.method().clone();

    sentry::configure_scope(|scope| {
        if let Some(span) = scope.get_span() {
            span.set_request(request.clone());
            if matched.is_none() {
                span.set_name(&format!("{method} {UNMATCHED_PATH}"));
            }
        }
        scope.add_event_processor(move |mut event| {
            event.request = Some(request.clone());
            Some(event)
        });
    });

    next.run(req).await
}

fn scrubbed_request(req: &Request<Body>, matched: Option<&str>) -> sentry::protocol::Request {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown");
    let path = matched.map_or_else(|| format!("/{UNMATCHED_PATH}"), str::to_owned);
    sentry::protocol::Request {
        url: format!("http://{host}{path}").parse().ok(),
        method: Some(req.method().to_string()),
        headers: FORWARDED_HEADERS
            .iter()
            .filter_map(|name| {
                let value = req.headers().get(name)?.to_str().ok()?;
                Some((name.to_string(), value.to_owned()))
            })
            .collect(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request<Body> {
        Request::builder()
            .uri("/v1/drive/share/tokSECRET?token=querySECRET")
            .header(header::HOST, "api.example.com")
            .header(header::USER_AGENT, "test-agent")
            .header(header::COOKIE, "session=cookieSECRET")
            .header(header::REFERER, "https://app.example.com/?token=refSECRET")
            .header("x-github-select-token", "headerSECRET")
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn scrubbed_request_uses_the_route_template_and_allowed_headers() {
        let scrubbed = scrubbed_request(&request(), Some("/v1/drive/share/{token}"));

        let rendered = format!("{scrubbed:?}");
        assert!(!rendered.contains("SECRET"), "{rendered}");
        assert_eq!(scrubbed.method.as_deref(), Some("GET"));
        assert_eq!(
            scrubbed.headers.get("user-agent").map(String::as_str),
            Some("test-agent")
        );
        assert!(scrubbed.url.is_some());
    }

    #[test]
    fn scrubbed_request_hides_unmatched_paths() {
        let scrubbed = scrubbed_request(&request(), None);

        let url = scrubbed.url.expect("url");
        assert_eq!(url.path(), "/unmatched");
        assert_eq!(url.query(), None);
    }
}

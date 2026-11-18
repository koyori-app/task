//! MCP の入口（`POST /mcp`、Streamable HTTP・状態なし・JSON 応答）。
//!
//! ツールは REST API を同じプロセスの中で呼び直す（`inner`）。仕様は docs/dev/features/mcp.md。

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{FromRequestParts, State},
    http::{HeaderValue, Request, StatusCode, header::WWW_AUTHENTICATE},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use common::error::AppError;
use common::settings::Settings;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use service::auth::AuthError;

use crate::AppState;
use crate::extractors::{AuthMethod, AuthUser, bearer_token_from_parts};
use crate::middlewares::logging::logging_middleware;

mod inner;
mod resolve;
mod tools;

/// `/mcp` を足したルーターを返す。
///
/// `api` は state とレイヤーを付け終えた REST API のルーター。複製をツールの内向きの呼び出し先に
/// する。`/mcp` 自体は Bearer 必須で Cookie を見ないので、CORS・CSRF・セッションのレイヤーの外に置く。
/// `OpenApiRouter` には載せない（`openapi.json` に出さない）。
pub fn mount(api: Router, state: AppState) -> Router {
    let inner = api.clone();
    let service = StreamableHttpService::new(
        move || Ok(tools::TaskMcp::new(inner.clone())),
        Arc::new(NeverSessionManager::default()),
        config(&state.settings),
    );
    let mcp = Router::new()
        .route_service("/mcp", service)
        .layer(middleware::from_fn_with_state(state, require_pat))
        .layer(middleware::from_fn(logging_middleware));
    api.merge(mcp)
}

fn config(settings: &Settings) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        // セッションを持たず、1 要求に JSON の 1 応答を返す（SSE を張らない）
        .with_legacy_session_mode(false)
        .with_json_response(true)
        // Host はリバースプロキシで変わる。資格情報は Bearer だけで、ブラウザーが勝手に付ける
        // ものではないので、DNS リバインディングの対象にならない（`middlewares::csrf` と同じ理由）
        .disable_allowed_hosts()
        // `Origin` 付きの要求（ブラウザー）はフロントの origin だけ通す。`Origin` の無い要求は通る
        .with_allowed_origins([origin_with_port(&settings.allow_origin)])
}

/// `{scheme}://{host}:{port}` の形に直す。ポートの無い項目は rmcp が非推奨の警告を出す。
/// 解析できない値はそのまま返す。
fn origin_with_port(origin: &str) -> String {
    let Ok(url) = reqwest::Url::parse(origin) else {
        return origin.to_string();
    };
    match (url.host_str(), url.port_or_known_default()) {
        (Some(host), Some(port)) => format!("{}://{host}:{port}", url.scheme()),
        _ => origin.to_string(),
    }
}

fn with_bearer_challenge(mut response: Response) -> Response {
    if response.status() == StatusCode::UNAUTHORIZED {
        response
            .headers_mut()
            .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    }
    response
}

/// PAT の Bearer だけを通す。
///
/// Bearer の無い要求はここで 401 にする。`AuthUser` へ進めるとセッション Cookie で認証されるため。
/// Device Token は scope を持たず、テナントの束縛も引けないので断る。
/// 通した `AuthUser` は使わない。内向きの呼び出しが毎回認証し直す。
async fn require_pat(State(state): State<AppState>, req: Request<Body>, next: Next) -> Response {
    let (mut parts, body) = req.into_parts();
    if bearer_token_from_parts(&parts).is_none() {
        return with_bearer_challenge(AuthError::Unauthorized.into_response());
    }
    let auth = match AuthUser::from_request_parts(&mut parts, &state).await {
        Ok(auth) => auth,
        Err(error) => return with_bearer_challenge(error.into_response()),
    };
    if !matches!(auth.method, AuthMethod::PersonalToken { .. }) {
        return AppError::Forbidden.into_response();
    }
    next.run(Request::from_parts(parts, body)).await
}

#[cfg(test)]
mod tests {
    use super::origin_with_port;

    #[test]
    fn origin_gets_the_default_port_of_its_scheme() {
        assert_eq!(
            origin_with_port("https://app.example.com"),
            "https://app.example.com:443"
        );
        assert_eq!(
            origin_with_port("http://app.example.com"),
            "http://app.example.com:80"
        );
    }

    #[test]
    fn explicit_port_is_kept() {
        assert_eq!(
            origin_with_port("http://localhost:3000"),
            "http://localhost:3000"
        );
        assert_eq!(
            origin_with_port("https://app.example.com:8443/"),
            "https://app.example.com:8443"
        );
    }

    #[test]
    fn unparsable_origin_is_returned_as_is() {
        assert_eq!(origin_with_port("not a url"), "not a url");
        assert_eq!(origin_with_port(""), "");
    }
}

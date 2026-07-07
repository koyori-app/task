//! Origin ヘッダによる CSRF 対策。
//!
//! 本番はセッション Cookie が `SameSite=None` のため、`multipart/form-data` のように
//! preflight なしで送れるリクエストはクロスサイトのフォームから発行できてしまう。
//! CORS はレスポンスの読み取りを防ぐだけで副作用の実行は防がないため、
//! 状態変更メソッドで Origin ヘッダが付いている場合は許可フロントエンド origin か
//! API 自身の origin（Host 一致。Scalar UI 等）以外を 403 で拒否する。
//! Origin なし（CLI・サーバー間・webhook）は素通しする。

use axum::{
    body::Body,
    extract::State,
    http::{Method, Request, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{AppState, error::AppError};

pub async fn csrf_origin_check(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }
    let Some(origin) = req.headers().get(header::ORIGIN) else {
        return next.run(req).await;
    };

    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .or_else(|| req.uri().authority().map(|a| a.as_str()));
    let allowed = origin
        .to_str()
        .ok()
        .is_some_and(|origin| origin_allowed(origin, &state.settings.allow_origin, host));

    if allowed {
        next.run(req).await
    } else {
        AppError::Forbidden.into_response()
    }
}

/// Origin が許可フロントエンド origin か、API 自身の origin（authority が Host と一致）か。
fn origin_allowed(origin: &str, allow_origin: &str, host: Option<&str>) -> bool {
    if origin == allow_origin {
        return true;
    }
    match (origin.split_once("://"), host) {
        (Some((_scheme, authority)), Some(host)) => authority == host,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOW: &str = "http://localhost:3000";

    #[test]
    fn allows_configured_frontend_origin() {
        assert!(origin_allowed(
            "http://localhost:3000",
            ALLOW,
            Some("localhost:3400")
        ));
    }

    #[test]
    fn allows_same_origin_as_api_host() {
        assert!(origin_allowed(
            "http://localhost:3400",
            ALLOW,
            Some("localhost:3400")
        ));
        assert!(origin_allowed(
            "https://api.example.com",
            "https://app.example.com",
            Some("api.example.com")
        ));
    }

    #[test]
    fn rejects_cross_site_origin() {
        assert!(!origin_allowed(
            "https://evil.example",
            ALLOW,
            Some("localhost:3400")
        ));
    }

    #[test]
    fn rejects_null_origin() {
        // サンドボックス化された iframe のフォーム送信は Origin: null になる
        assert!(!origin_allowed("null", ALLOW, Some("localhost:3400")));
    }

    #[test]
    fn rejects_when_host_unknown() {
        assert!(!origin_allowed("http://localhost:3400", ALLOW, None));
    }

    #[test]
    fn rejects_port_mismatch() {
        assert!(!origin_allowed(
            "http://localhost:9999",
            ALLOW,
            Some("localhost:3400")
        ));
    }
}

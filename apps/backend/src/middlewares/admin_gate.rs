//! 運用系ルート（Apalis board 等）を管理者セッションに限定するミドルウェア。
//!
//! board のジョブペイロードにはメール認証トークンやメールアドレスが含まれるため、
//! 公開 API と同じリスナーに載せる以上、管理者以外には一切見せない。

use axum::{
    body::Body,
    extract::{FromRequestParts, State},
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{AppState, extractors::AdminUser};

pub async fn require_admin_session(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let (mut parts, body) = req.into_parts();
    match AdminUser::from_request_parts(&mut parts, &state).await {
        Ok(_) => next.run(Request::from_parts(parts, body)).await,
        Err(e) => e.into_response(),
    }
}

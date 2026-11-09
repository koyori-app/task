use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};

use crate::AppState;
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use service::favicon::{FALLBACK_MIME, FALLBACK_SVG, Favicon, favicon_for_host, parse_host};

/// 本文の外部リンクの先頭に出す favicon をサーバーが取って配信する（TASK-362）。
///
/// 認証を必須にする理由: host は誰でも列挙できる値で、無認証だと誰でもこのサーバーを
/// 「任意ホストへの GET」の踏み台にできる。`<img>` は same-origin（`/api` プロキシ経由）なので
/// Cookie が付き、ログイン中の画面からは問題なく読める。
/// `Cache-Control` を `private` にするのは、認証付きの応答を共有キャッシュに置かせないため。
/// 取れない時は 404 ではなく 200 で汎用アイコンを返す（404 はブラウザがキャッシュしにくく、
/// 閲覧のたびに再要求になる）。
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{host}",
    tag = "Tasks",
    summary = "リンク先サイトの favicon を配信",
    params(("host" = String, Path, description = "小文字のホスト名（ドット必須、253 文字以下）")),
    responses(
        (status = 200, description = "favicon。取れないときは汎用アイコン（image/svg+xml）", content(
            ("image/x-icon"), ("image/png"), ("image/jpeg"), ("image/gif"), ("image/webp"),
            ("image/svg+xml")
        )),
        (status = 401, description = "未認証", body = ServerError),
        (status = 404, description = "host の形式が不正", body = ServerError),
    )
)]
pub async fn get_favicon(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(host): Path<String>,
) -> Result<Response, AppError> {
    let host = parse_host(&host).ok_or(AppError::NotFound)?;
    let favicon = favicon_for_host(&state.redis_client, &host)
        .await
        .map_err(AppError::Internal)?;

    let (mime, cache_control, body) = match favicon {
        Favicon::Image { mime, bytes } => (mime, "private, max-age=86400", Body::from(bytes)),
        Favicon::Fallback { cacheable: true } => (
            FALLBACK_MIME,
            "private, max-age=3600",
            Body::from(FALLBACK_SVG),
        ),
        Favicon::Fallback { cacheable: false } => {
            (FALLBACK_MIME, "no-store", Body::from(FALLBACK_SVG))
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache_control)
        .header(header::CONTENT_DISPOSITION, "inline")
        .body(body)
        .map_err(|e| AppError::Internal(e.into()))
}

use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};

use crate::AppState;
use crate::error::{AppError, ServerError};
use service::avatar::avatar_key_mime;
use service::storage::StorageError;

/// `<img src>` から読めるよう認証なしで配信する。キーは UUID v4 で推測できず、
/// アップロード時に形式を確かめた画像だけが置かれる。DB は引かない。
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{key}",
    tag = "Auth",
    summary = "アバター画像配信",
    params(("key" = String, Path, description = "アバターのキー（avatar-{uuid}.{png|jpg|gif|webp}）")),
    responses(
        (status = 200, description = "画像バイナリ", content(
            ("image/png"), ("image/jpeg"), ("image/gif"), ("image/webp")
        )),
        (status = 404, description = "キーの形式が不正か、画像がありません", body = ServerError),
    )
)]
pub async fn get_avatar(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Response, AppError> {
    let mime = avatar_key_mime(&key).ok_or(AppError::NotFound)?;
    let stream = state.storage.get_stream(&key).await.map_err(|e| match e {
        StorageError::NotFound => AppError::NotFound,
        e => AppError::Internal(e.into()),
    })?;

    // 差し替え時はキーごと変わるので、同じ URL の中身は変わらない
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header(header::CONTENT_DISPOSITION, "inline")
        .body(Body::from_stream(stream))
        .map_err(|e| AppError::Internal(e.into()))
}

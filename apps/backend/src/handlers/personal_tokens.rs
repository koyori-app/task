use serde::Deserialize;
use validator::Validate;

use crate::dto::personal_tokens::{CreatePersonalTokenResponse, PersonalTokenResponse};

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
struct CreatePersonalTokenRequest {
    // フィールドは後で定義
}

// 対象ユーザーの新しいパーソナルアクセストークンを作成する
#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    request_body = CreatePersonalTokenRequest,
    responses(
        (status = 200, description = "Personal token created", body = CreatePersonalTokenResponse)
    )
)]
pub async fn create_personal_token() {
    todo!()
}

// 対象ユーザーの特定のパーソナルアクセストークンを取得する
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    params(("id" = String, Path, description = "Personal token ID")),
    responses(
        (status = 200, description = "Personal token found", body = PersonalTokenResponse)
    )
)]
pub async fn get_personal_token() {
    todo!()
}

// 対象ユーザーの特定のパーソナルアクセストークンを失効させる
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    params(("id" = String, Path, description = "Personal token ID")),
    responses(
        (status = 200, description = "Personal token revoked", body = PersonalTokenResponse)
    )
)]
pub async fn revoke_personal_token() {
    todo!()
}

// 対象ユーザーの全てのパーソナルアクセストークンを失効させる
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/",
    responses(
        (status = 200, description = "All personal tokens revoked", body = [PersonalTokenResponse])
    )
)]
pub async fn revoke_all_personal_tokens() {
    todo!()
}

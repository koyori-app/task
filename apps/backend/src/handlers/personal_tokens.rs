use serde::Deserialize;
use validator::Validate;

use crate::entities;

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
struct CreatePersonalTokenRequest {
    // フィールドは後で定義
}

// 対象ユーザーの新しいパーソナルアクセストークンを作成する
#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/personal_tokens",
    request_body = CreatePersonalTokenRequest,
    responses(
        (status = 200, description = "Personal token created", body = entities::personal_tokens::Model)
    )
)]
pub async fn create_personal_token() {
    todo!()
}

// 対象ユーザーの特定のパーソナルアクセストークンを取得する
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/personal_tokens/{id}",
    responses(
        (status = 200, description = "Personal token found", body = entities::personal_tokens::Model)
    )
)]
pub async fn get_personal_token() {
    todo!()
}

// 対象ユーザーの特定のパーソナルアクセストークンを失効させる
#[axum::debug_handler] 
#[utoipa::path(
    delete,
    path = "/personal_tokens/{id}",
    responses(
        (status = 200, description = "Personal token revoked", body = entities::personal_tokens::Model)
    )
)]
pub async fn revoke_personal_token() {
    todo!()
}

// 対象ユーザーの全てのパーソナルアクセストークンを失効させる
#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/personal_tokens",
    responses(
        (status = 200, description = "All personal tokens revoked", body = Vec<entities::personal_tokens::Model>)
    )
)]
pub async fn revoke_all_personal_tokens() {
    todo!()
}
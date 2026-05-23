use axum::extract::Path;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::dto::personal_tokens::{CreatePersonalTokenResponse, PersonalTokenResponse};
use crate::openapi::SessionAuthErrors;

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
struct CreatePersonalTokenRequest {}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    summary = "パーソナルアクセストークンを発行",
    request_body = CreatePersonalTokenRequest,
    responses(
        (
            status = 200,
            description = "発行したトークンの情報",
            body = CreatePersonalTokenResponse
        ),
        SessionAuthErrors,
    )
)]
pub async fn create_personal_token() {
    todo!()
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    summary = "指定したトークンを参照",
    params(("id" = Uuid, Path, description = "トークンの識別子")),
    responses(
        (
            status = 200,
            description = "トークンの状態",
            body = PersonalTokenResponse
        ),
        SessionAuthErrors,
    )
)]
pub async fn get_personal_token(Path(_id): Path<Uuid>) {
    todo!()
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    summary = "指定したトークンを取り消し",
    params(("id" = Uuid, Path, description = "トークンの識別子")),
    responses(
        (
            status = 200,
            description = "取り消し後の状態",
            body = PersonalTokenResponse
        ),
        SessionAuthErrors,
    )
)]
pub async fn revoke_personal_token(Path(_id): Path<Uuid>) {
    todo!()
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/",
    summary = "すべての個人用トークンを取り消し",
    responses(
        (
            status = 200,
            description = "現在アクティブなトークンの一覧（空になり得ます）",
            body = [PersonalTokenResponse]
        ),
        SessionAuthErrors,
    )
)]
pub async fn revoke_all_personal_tokens() {
    todo!()
}

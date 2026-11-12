//! テナント管理者が所有する PAT 専用アカウント。
use crate::auth_helpers::require_tenant_admin;
use crate::{AppState, error::AppError, extractors::AuthUser};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use payload::{
    bots::*,
    personal_tokens::{CreatePersonalTokenResponse, PersonalTokenResponse},
};
use sea_orm::prelude::Uuid;

async fn authorize(state: &AppState, auth: &AuthUser, tenant_id: Uuid) -> Result<(), AppError> {
    auth.require_session()?;
    // tenant_members と同じく、所属とテナントの 2FA 強制を先に確かめる
    auth.ensure_tenant_access(state, tenant_id, None).await?;
    require_tenant_admin(&state.db, tenant_id, auth.user_id).await
}

#[utoipa::path(get, path = "/{tenant_id}/bots", tag = "Bots",
    params(("tenant_id" = Uuid, Path)), responses((status = 200, body = Vec<BotResponse>), crate::openapi::SessionAuthErrors))]
pub async fn list_bots(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<BotResponse>>, AppError> {
    authorize(&state, &auth, tenant_id).await?;
    service::bots::list_bots(&state.db, tenant_id, auth.user_id)
        .await
        .map(Json)
}

#[utoipa::path(post, path = "/{tenant_id}/bots", tag = "Bots", request_body = CreateBotRequest,
    params(("tenant_id" = Uuid, Path)), responses((status = 201, body = BotResponse), crate::openapi::SessionAuthErrors))]
pub async fn create_bot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<CreateBotRequest>>,
) -> Result<(StatusCode, Json<BotResponse>), AppError> {
    authorize(&state, &auth, tenant_id).await?;
    service::bots::create_bot(&state.db, tenant_id, auth.user_id, payload)
        .await
        .map(|result| (StatusCode::CREATED, Json(result)))
}

#[utoipa::path(patch, path = "/{tenant_id}/bots/{bot_id}", tag = "Bots", request_body = UpdateBotRequest,
    params(("tenant_id" = Uuid, Path), ("bot_id" = Uuid, Path)), responses((status = 204), crate::openapi::SessionAuthErrors))]
pub async fn update_bot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, bot_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateBotRequest>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &auth, tenant_id).await?;
    service::bots::update_bot(&state.db, tenant_id, bot_id, auth.user_id, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/{tenant_id}/bots/{bot_id}/tokens", tag = "Bots",
    params(("tenant_id" = Uuid, Path), ("bot_id" = Uuid, Path)), responses((status = 200, body = Vec<PersonalTokenResponse>), crate::openapi::SessionAuthErrors))]
pub async fn list_bot_tokens(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, bot_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<PersonalTokenResponse>>, AppError> {
    authorize(&state, &auth, tenant_id).await?;
    service::bots::list_bot_tokens(&state.db, tenant_id, bot_id, auth.user_id)
        .await
        .map(Json)
}

#[utoipa::path(post, path = "/{tenant_id}/bots/{bot_id}/tokens", tag = "Bots", request_body = CreateBotTokenRequest,
    params(("tenant_id" = Uuid, Path), ("bot_id" = Uuid, Path)), responses((status = 201, body = CreatePersonalTokenResponse), crate::openapi::SessionAuthErrors))]
pub async fn create_bot_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, bot_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateBotTokenRequest>>,
) -> Result<(StatusCode, Json<CreatePersonalTokenResponse>), AppError> {
    authorize(&state, &auth, tenant_id).await?;
    service::bots::create_bot_token(
        &state.db,
        &state.settings.personal_token_secret,
        tenant_id,
        bot_id,
        auth.user_id,
        payload,
    )
    .await
    .map(|result| (StatusCode::CREATED, Json(result)))
}

#[utoipa::path(delete, path = "/{tenant_id}/bots/{bot_id}/tokens/{token_id}", tag = "Bots",
    params(("tenant_id" = Uuid, Path), ("bot_id" = Uuid, Path), ("token_id" = Uuid, Path)), responses((status = 204), crate::openapi::SessionAuthErrors))]
pub async fn revoke_bot_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, bot_id, token_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &auth, tenant_id).await?;
    service::bots::revoke_bot_token(&state.db, tenant_id, bot_id, token_id, auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

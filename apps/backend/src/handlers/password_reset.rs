use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use axum_session_redispool::SessionRedisPool;
use axum_valid::Valid;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tracing::warn;
use validator::Validate;

use crate::extractors::{AuthMethod, AuthUser, CurrentUser};
use crate::jobs::{PasswordResetEmailJob, password_reset_email};
use crate::openapi::{
    PasswordChangeErrors, PasswordResetCompleteErrors, PasswordResetRequestErrors,
    PasswordResetVerifyErrors,
};
use crate::utils::auth::{
    AuthError, DUMMY_PASSWORD_HASH, create_password_hash, generate_email_verification_token,
    verify_password,
};
use crate::utils::email::normalize_email;
use crate::utils::password_reset;
use crate::{AppState, entities::users};

type AuthSession = axum_session::Session<SessionRedisPool>;

#[derive(Serialize, utoipa::ToSchema)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct PasswordResetRequestBody {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PasswordResetVerifyQuery {
    pub token: String,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct PasswordResetCompleteBody {
    #[validate(length(min = 1))]
    pub token: String,
    #[validate(length(min = 8))]
    pub new_password: String,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct PasswordChangeBody {
    pub current_password: String,
    #[validate(length(min = 8))]
    pub new_password: String,
}

#[utoipa::path(
    post,
    path = "/password-reset/request",
    tag = "Auth",
    request_body = PasswordResetRequestBody,
    responses((status = 200, body = MessageResponse), PasswordResetRequestErrors)
)]
pub async fn password_reset_request(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<PasswordResetRequestBody>>,
) -> Result<Json<MessageResponse>, AuthError> {
    let email = normalize_email(&payload.email);
    if !password_reset::try_acquire_rate_limit(&state.redis_client, &email)
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
    {
        return Err(AuthError::TooManyRequests);
    }
    let _ = verify_password("timing-normalize", DUMMY_PASSWORD_HASH)?;
    if let Some(user) = users::Entity::find()
        .filter(users::Column::Email.eq(&email))
        .one(&state.db)
        .await?
    {
        let token = generate_email_verification_token();
        if let Err(e) = password_reset::store_token(&state.redis_client, user.id, &token).await {
            warn!(user_id = %user.id, error = ?e, "password reset token store failed");
        } else if let Err(e) = password_reset_email::enqueue(
            state.password_reset_email_storage.as_ref(),
            PasswordResetEmailJob::new(user.id, email),
        )
        .await
        {
            warn!(user_id = %user.id, error = ?e, "password reset email enqueue failed");
        }
    }
    Ok(Json(MessageResponse {
        message: "入力されたメールアドレスにリセットリンクを送信しました（登録済みの場合）".into(),
    }))
}

#[utoipa::path(
    get,
    path = "/password-reset/verify",
    tag = "Auth",
    params(PasswordResetVerifyQuery),
    responses((status = 200), PasswordResetVerifyErrors)
)]
pub async fn password_reset_verify(
    State(state): State<AppState>,
    Query(query): Query<PasswordResetVerifyQuery>,
) -> Result<StatusCode, AuthError> {
    let exists = password_reset::token_exists(&state.redis_client, &query.token)
        .await
        .map_err(|e| AuthError::Internal(e.into()))?;
    if exists {
        Ok(StatusCode::OK)
    } else {
        Err(AuthError::PasswordResetTokenNotFound)
    }
}

#[utoipa::path(
    post,
    path = "/password-reset/complete",
    tag = "Auth",
    request_body = PasswordResetCompleteBody,
    responses((status = 200, body = MessageResponse), PasswordResetCompleteErrors)
)]
pub async fn password_reset_complete(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<PasswordResetCompleteBody>>,
) -> Result<Json<MessageResponse>, AuthError> {
    if payload.new_password.len() < 8 {
        return Err(AuthError::InvalidNewPassword);
    }
    if !password_reset::token_exists(&state.redis_client, &payload.token)
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
    {
        return Err(AuthError::InvalidPasswordResetToken);
    }
    let password_hash = create_password_hash(&payload.new_password)?;
    let user_id = password_reset::consume_token(&state.redis_client, &payload.token)
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
        .ok_or(AuthError::InvalidPasswordResetToken)?;
    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AuthError::Internal(anyhow::anyhow!("user missing for token")))?;
    let mut active: users::ActiveModel = user.into();
    active.password_hash = Set(password_hash);
    active.sessions_revoked_at = Set(Some(Utc::now().fixed_offset()));
    active.update(&state.db).await?;
    Ok(Json(MessageResponse {
        message: "パスワードをリセットしました。再度ログインしてください。".into(),
    }))
}

#[utoipa::path(
    post,
    path = "/password/change",
    tag = "Auth",
    request_body = PasswordChangeBody,
    responses((status = 200, body = MessageResponse), PasswordChangeErrors)
)]
pub async fn password_change(
    session: AuthSession,
    State(state): State<AppState>,
    auth: AuthUser,
    user: CurrentUser,
    Valid(Json(payload)): Valid<Json<PasswordChangeBody>>,
) -> Result<Json<MessageResponse>, AuthError> {
    if !matches!(auth.method, AuthMethod::Session) {
        return Err(AuthError::Unauthorized);
    }
    if user.password_hash.is_empty() {
        return Err(AuthError::PasswordNotSet);
    }
    if !verify_password(&payload.current_password, &user.password_hash)? {
        return Err(AuthError::InvalidCurrentPassword);
    }
    if payload.new_password.len() < 8 {
        return Err(AuthError::InvalidNewPassword);
    }
    let password_hash = create_password_hash(&payload.new_password)?;
    let mut active: users::ActiveModel = user.0.clone().into();
    active.password_hash = Set(password_hash);
    active.sessions_revoked_at = Set(Some(Utc::now().fixed_offset()));
    active.update(&state.db).await?;
    session.remove("user_id");
    session.remove("issued_at_ms");
    Ok(Json(MessageResponse {
        message: "パスワードを変更しました。再度ログインしてください。".into(),
    }))
}

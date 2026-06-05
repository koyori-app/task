use axum::{
    Json,
    extract::State,
    http::StatusCode,
};
use axum_session_redispool::SessionRedisPool;
use axum_valid::Valid;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use sea_orm::sea_query::Expr;
use serde::{Deserialize, Serialize};
use tracing::warn;
use validator::Validate;

use crate::extractors::CurrentUser;
use crate::jobs::{PasswordResetEmailJob, password_reset_email};
use crate::openapi::{
    PasswordChangeErrors, PasswordResetCompleteErrors, PasswordResetRequestErrors,
    PasswordResetVerifyErrors,
};
use crate::utils::auth::{AuthError, create_password_hash, verify_password};
use crate::utils::email::normalize_email;
use crate::utils::{password_reset, password_reset_log};
use crate::{AppState, entities::{personal_tokens, users}};

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

#[derive(Validate, Deserialize, utoipa::ToSchema)]
pub struct PasswordResetVerifyBody {
    #[validate(length(min = 1))]
    pub token: String,
}

#[derive(Validate, Deserialize, utoipa::ToSchema)]
pub struct PasswordResetCompleteBody {
    #[validate(length(min = 1))]
    pub token: String,
    #[validate(length(min = 8))]
    pub new_password: String,
}

#[derive(Validate, Deserialize, utoipa::ToSchema)]
pub struct PasswordChangeBody {
    pub current_password: String,
    #[validate(length(min = 8))]
    pub new_password: String,
}

impl std::fmt::Debug for PasswordResetVerifyBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordResetVerifyBody")
            .field("token", &"<redacted>")
            .finish()
    }
}

impl std::fmt::Debug for PasswordResetCompleteBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordResetCompleteBody")
            .field("token", &"<redacted>")
            .field("new_password", &"<redacted>")
            .finish()
    }
}

impl std::fmt::Debug for PasswordChangeBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordChangeBody")
            .field("current_password", &"<redacted>")
            .field("new_password", &"<redacted>")
            .finish()
    }
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
    if let Some(user) = users::Entity::find()
        .filter(users::Column::Email.eq(&email))
        .one(&state.db)
        .await?
    {
        if user.is_suspended {
            return Ok(Json(MessageResponse {
                message: "入力されたメールアドレスにリセットリンクを送信しました（登録済みの場合）".into(),
            }));
        }
        if let Err(e) = password_reset_email::enqueue(
            state.password_reset_email_storage.as_ref(),
            PasswordResetEmailJob::new(user.id, email),
        )
        .await
        {
            warn!(user_id = %user.id, error = ?e, "password reset email enqueue failed");
        } else {
            password_reset_log::email_queued(user.id);
        }
    }
    Ok(Json(MessageResponse {
        message: "入力されたメールアドレスにリセットリンクを送信しました（登録済みの場合）".into(),
    }))
}

#[utoipa::path(
    post,
    path = "/password-reset/verify",
    tag = "Auth",
    request_body = PasswordResetVerifyBody,
    responses((status = 200), PasswordResetVerifyErrors)
)]
pub async fn password_reset_verify(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<PasswordResetVerifyBody>>,
) -> Result<StatusCode, AuthError> {
    let valid = password_reset::lookup_token_user_id(&state.redis_client, &payload.token)
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
        .is_some();
    if valid {
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
    let user_id = password_reset::consume_token(&state.redis_client, &payload.token)
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
        .ok_or(AuthError::InvalidPasswordResetToken)?;
    let password_hash = create_password_hash(&payload.new_password)?;

    let txn = state
        .db
        .begin()
        .await
        .map_err(|e| AuthError::Internal(e.into()))?;
    let user = users::Entity::find_by_id(user_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AuthError::Internal(anyhow::anyhow!("user missing for token")))?;
    let mut active: users::ActiveModel = user.into();
    active.password_hash = Set(Some(password_hash));
    active.sessions_revoked_at = Set(Some(Utc::now()));
    active.update(&txn).await?;
    personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::Revoked, Expr::value(true))
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .exec(&txn)
        .await?;
    txn.commit()
        .await
        .map_err(|e| AuthError::Internal(e.into()))?;

    password_reset_log::reset_completed(user_id);
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
    user: CurrentUser,
    Valid(Json(payload)): Valid<Json<PasswordChangeBody>>,
) -> Result<Json<MessageResponse>, AuthError> {
    let current_hash = user
        .password_hash
        .as_deref()
        .ok_or(AuthError::PasswordNotSet)?;
    if !verify_password(&payload.current_password, current_hash)? {
        return Err(AuthError::InvalidCurrentPassword);
    }
    let user_id = user.id;
    let password_hash = create_password_hash(&payload.new_password)?;
    let mut active: users::ActiveModel = user.0.into();
    active.password_hash = Set(Some(password_hash));
    active.sessions_revoked_at = Set(Some(Utc::now()));

    let txn = state
        .db
        .begin()
        .await
        .map_err(|e| AuthError::Internal(e.into()))?;
    active.update(&txn).await?;
    personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::Revoked, Expr::value(true))
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .exec(&txn)
        .await?;
    txn.commit()
        .await
        .map_err(|e| AuthError::Internal(e.into()))?;
    session.remove("user_id");
    session.remove("issued_at_ms");
    password_reset_log::password_changed(user_id);
    Ok(Json(MessageResponse {
        message: "パスワードを変更しました。再度ログインしてください。".into(),
    }))
}

use axum::{Json, extract::State, http::StatusCode};
use axum_session::Session;
use axum_session_redispool::SessionRedisPool;
use axum_valid::Valid;
use sea_orm::prelude::Uuid;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use sea_orm::{ColumnTrait, QueryFilter};
use serde::Deserialize;
use validator::Validate;

use crate::entities;
use crate::extractors::{AuthUser, CurrentUser};
use crate::openapi::{
    CredentialErrors, RegisterErrors, ResendVerificationErrors, SessionAuthErrors,
    UnauthorizedErrors, VerifyEmailErrors,
};
use crate::settings::Settings;
use crate::utils::auth::{
    AuthError, create_password_hash, generate_email_verification_token, verify_password,
};
use crate::utils::db::is_postgres_unique_violation;
use crate::utils::{email_verification, smtp::SmtpClient};
use crate::{AppState, entities::users};

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    #[schema(value_type = String, format="email")]
    #[validate(email)]
    pub email: String,
    #[schema(value_type = String, format="password")]
    #[validate(length(min = 8))]
    pub password: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/login",
    summary = "ログイン",
    request_body = LoginRequest,
    responses(
        (status = 204, description = "ログインに成功しました（本文なし）"),
        CredentialErrors,
    )
)]
pub async fn login(
    session: Session<SessionRedisPool>,
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<LoginRequest>>,
) -> Result<StatusCode, AuthError> {
    let LoginRequest { email, password } = payload;

    let user = users::Entity::find()
        .filter(users::Column::Email.eq(email))
        .one(&state.db)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;
    if !verify_password(&password, &user.password_hash)? {
        return Err(AuthError::InvalidCredentials);
    }
    if !user.email_verified {
        return Err(AuthError::EmailNotVerified);
    }

    session.set("user_id", user.id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    #[schema(value_type = String, format="username")]
    #[validate(length(min = 3))]
    pub username: String,
    #[schema(value_type = String, format="email")]
    #[validate(email)]
    pub email: String,
    #[schema(value_type = String, format="password")]
    #[validate(length(min = 8))]
    pub password: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/register",
    summary = "新規登録",
    request_body = RegisterRequest,
    responses(
        (
            status = 201,
            description = "アカウントが作成されました。続けて送信されたメールで認証してください。",
            body = String
        ),
        RegisterErrors,
    )
)]
pub async fn register(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<RegisterRequest>>,
) -> Result<(StatusCode, Json<String>), AuthError> {
    let RegisterRequest {
        username,
        email,
        password,
    } = payload;

    let password_hash = create_password_hash(&password)?;
    let verification_token = generate_email_verification_token();
    let user_id = Uuid::new_v4();

    let user = users::ActiveModel {
        id: Set(user_id),
        username: Set(username),
        bio: Set(Some(String::new())),
        avatar_url: Set(None),
        email: Set(email.clone()),
        email_verified: Set(false),
        password_hash: Set(password_hash),
    };

    users::Entity::insert(user.clone())
        .exec(&state.db)
        .await
        .map_err(|e| {
            if is_postgres_unique_violation(&e) {
                AuthError::DuplicateEmail
            } else {
                AuthError::Internal(anyhow::anyhow!("insert user: {e}"))
            }
        })?;

    email_verification::store_token(&state.redis_client, user_id, &verification_token)
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis store verification token: {e}")))?;

    send_verification_email(
        &state.smtp_client,
        &email,
        &state.settings,
        &verification_token,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json("Register successful".to_string()),
    ))
}

/// メールでの本人確認時に送信する情報。
#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct VerifyEmailRequest {
    /// メールまたはアプリにお知らせした認証用文字列です。
    #[validate(length(min = 1))]
    pub token: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/verify-email",
    summary = "メールアドレスの確認",
    request_body = VerifyEmailRequest,
    responses(
        (
            status = 200,
            description = "メールアドレスの確認が完了しました",
            body = String
        ),
        VerifyEmailErrors,
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<VerifyEmailRequest>>,
) -> Result<Json<String>, AuthError> {
    let user_id =
        email_verification::consume_token(&state.redis_client, &payload.token)
            .await
            .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis consume verification token: {e}")))?
            .ok_or(AuthError::InvalidVerificationToken)?;

    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| {
            AuthError::Internal(anyhow::anyhow!(
                "email verification token referenced missing user"
            ))
        })?;

    if user.email_verified {
        return Ok(Json("Email already verified".to_string()));
    }

    let mut active: users::ActiveModel = user.into();
    active.email_verified = Set(true);
    active.update(&state.db).await?;

    Ok(Json("Email verified".to_string()))
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct ResendVerificationRequest {
    #[schema(value_type = String, format="email")]
    #[validate(email)]
    pub email: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/resend-verification-email",
    summary = "認証メールの再送",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, description = "認証メールを送信しました", body = String),
        ResendVerificationErrors,
    )
)]
pub async fn resend_verification_email(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<ResendVerificationRequest>>,
) -> Result<Json<String>, AuthError> {
    let email = payload.email.trim().to_string();

    if !email_verification::try_acquire_resend_slot(&state.redis_client, &email)
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis resend cooldown: {e}")))?
    {
        return Err(AuthError::TooManyRequests);
    }

    let user = users::Entity::find()
        .filter(users::Column::Email.eq(email.clone()))
        .one(&state.db)
        .await?
        .ok_or(AuthError::UserNotFound)?;

    if user.email_verified {
        return Err(AuthError::EmailAlreadyVerified);
    }

    let token = generate_email_verification_token();
    email_verification::store_token(&state.redis_client, user.id, &token)
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis store verification token: {e}")))?;

    send_verification_email(&state.smtp_client, &email, &state.settings, &token).await?;

    Ok(Json(format!(
        "確認メールを再送しました（同一メールアドレスへの再送は{}秒に1回までです）。",
        email_verification::RESEND_COOLDOWN_SECS
    )))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/me",
    summary = "ログイン中ユーザー情報",
    responses(
        (status = 200, description = "現在のアカウント情報", body = entities::users::Model),
        SessionAuthErrors,
    )
)]
pub async fn me(
    State(_): State<AppState>,
    user: CurrentUser,
) -> Result<Json<entities::users::Model>, AuthError> {
    Ok(Json(user.0))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/logout",
    summary = "ログアウト",
    responses(
        (status = 204, description = "ログアウトしました（本文なし）"),
        UnauthorizedErrors,
    )
)]
pub async fn logout(
    session: Session<SessionRedisPool>,
    State(_): State<AppState>,
    _auth: AuthUser,
) -> Result<StatusCode, AuthError> {
    session.remove("user_id");
    Ok(StatusCode::NO_CONTENT)
}

fn build_verify_url(settings: &Settings, token: &str) -> String {
    let encoded = urlencoding::encode(token);
    format!(
        "{}/verify-email?token={}",
        settings.email_verification_app_url.trim_end_matches('/'),
        encoded
    )
}

async fn send_verification_email(
    smtp: &SmtpClient,
    email: &str,
    settings: &Settings,
    token: &str,
) -> Result<(), AuthError> {
    let verify_url = build_verify_url(settings, token);
    let mins = email_verification::TOKEN_TTL_SECS / 60;
    smtp.send_email(
        email,
        "メール認証",
        &format!(
            "以下のリンクからアプリを開き、表示に従ってメールアドレスの確認を完了してください（有効期限は約{mins}分です）。\n{verify_url}",
        ),
        Some(&format!(
            "<p>以下のリンクからアプリを開き、表示に従ってメールアドレスの確認を完了してください（有効期限は約{mins}分です）。</p><p><a href=\"{verify_url}\">{verify_url}</a></p>",
        )),
    )
    .await
    .map_err(|e| AuthError::Internal(anyhow::anyhow!("send verification email: {e}")))?;
    Ok(())
}

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
use crate::utils::auth::{
    AuthError, DUMMY_PASSWORD_HASH, create_password_hash, generate_email_verification_token,
    verify_password,
};
use crate::jobs::VerificationEmailJob;
use crate::jobs::verification_email;
use crate::utils::db::{is_postgres_unique_violation, with_transaction};
use crate::utils::email::normalize_email;
use crate::utils::email_verification;
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
    let email = normalize_email(&email);

    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&email))
        .one(&state.db)
        .await?;

    let password_hash = user
        .as_ref()
        .map(|u| u.password_hash.as_str())
        .unwrap_or(DUMMY_PASSWORD_HASH);

    if !verify_password(&password, password_hash)? {
        return Err(AuthError::InvalidCredentials);
    }

    let user = user.ok_or(AuthError::InvalidCredentials)?;

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
    let email = normalize_email(&email);

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

    with_transaction::<(), AuthError, _>(&state.db, |txn| {
        Box::pin(async move {
            users::Entity::insert(user.clone())
                .exec(txn)
                .await
                .map_err(|e| {
                    if is_postgres_unique_violation(&e) {
                        AuthError::DuplicateEmail
                    } else {
                        AuthError::Internal(anyhow::anyhow!("insert user: {e}"))
                    }
                })?;

            Ok(())
        })
    })
    .await?;

    verification_email::enqueue(
        state.verification_email_storage.as_ref(),
        VerificationEmailJob {
            user_id,
            email: email.clone(),
            token: verification_token,
        },
    )
    .await
    .map_err(|e| AuthError::Internal(anyhow::anyhow!("enqueue verification email: {e}")))?;
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
    let email = normalize_email(&payload.email);

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
    verification_email::enqueue(
        state.verification_email_storage.as_ref(),
        VerificationEmailJob {
            user_id: user.id,
            email: email.clone(),
            token,
        },
    )
    .await
    .map_err(|e| AuthError::Internal(anyhow::anyhow!("enqueue verification email: {e}")))?;

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


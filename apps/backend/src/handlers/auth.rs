use axum::{Json, extract::State};
use axum_session::Session;
use axum_session_redispool::SessionRedisPool;
use axum_valid::Valid;
use sea_orm::prelude::Uuid;
use sea_orm::{ActiveValue::Set, EntityTrait};
use sea_orm::{ColumnTrait, QueryFilter};
use serde::Deserialize;
use validator::Validate;

use crate::entities;
use crate::extractors::{AuthUser, CurrentUser};
use crate::openapi::{CredentialErrors, InternalOnlyError, SessionAuthErrors, UnauthorizedErrors};
use crate::utils::auth::{AuthError, create_password_hash, verify_password};
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
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = String),
        CredentialErrors,
    )
)]
pub async fn login(
    session: Session<SessionRedisPool>,
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<LoginRequest>>,
) -> Result<Json<String>, AuthError> {
    let LoginRequest { email, password } = payload;

    let user = users::Entity::find()
        .filter(users::Column::Email.eq(email))
        .one(&state.db)
        .await?
        .ok_or(AuthError::Forbidden)?;
    if verify_password(&password, &user.password_hash)? {
        session.set("user_id", user.id);
        Ok(Json("Login successful".to_string()))
    } else {
        Err(AuthError::Forbidden)
    }
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
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Register successful", body = String),
        InternalOnlyError,
    )
)]
pub async fn register(
    session: Session<SessionRedisPool>,
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<RegisterRequest>>,
) -> Result<Json<String>, AuthError> {
    let RegisterRequest {
        username,
        email,
        password,
    } = payload;

    let password_hash = create_password_hash(&password)?;
    let user_id = Uuid::new_v4();

    let user = users::ActiveModel {
        id: Set(user_id),
        username: Set(username),
        bio: Set(Some(String::new())),
        avatar_url: Set(None),
        email: Set(email),
        password_hash: Set(password_hash),
    };

    users::Entity::insert(user.clone())
        .exec(&state.db)
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("insert user: {e}")))?;

    session.set("user_id", user_id);
    Ok(Json("Register successful".to_string()))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/me",
    responses(
        (status = 200, description = "Current user info", body = entities::users::Model),
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
    responses(
        (status = 200, description = "Logout successful", body = String),
        UnauthorizedErrors,
    )
)]
pub async fn logout(
    session: Session<SessionRedisPool>,
    State(_): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<String>, AuthError> {
    session.remove("user_id");
    Ok(Json("Logout successful".to_string()))
}

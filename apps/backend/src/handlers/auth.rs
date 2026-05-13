use axum::{extract::State, Json};
use axum_session::Session;
use axum_session_redispool::SessionRedisPool;
use axum_valid::Valid;
use sea_orm::{ColumnTrait, QueryFilter};
use sea_orm::{ActiveValue::Set, EntityTrait};
use sea_orm::prelude::Uuid;
use serde::Deserialize;
use validator::Validate;

use crate::entities;
use crate::utils::auth::{AuthError, create_password_hash};
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
        (status = 200, description = "Login successful", body = String)
    )
)]
pub async fn login(session: Session<SessionRedisPool>, State(state): State<AppState>, Valid(Json(payload)): Valid<Json<LoginRequest>>) -> Result<Json<String>, AuthError> {
    let LoginRequest { email, password } = payload;

    let user = users::Entity::find().filter(users::Column::Email.eq(email)).one(&state.db).await.unwrap();
    let password_hash = create_password_hash(&password);

    if let (Some(user), Ok(password_hash)) = (user, password_hash) {
        if user.password_hash.as_deref() == Some(&password_hash) {
            session.set("user_id", user.id);
            return Ok(Json("Login successful".to_string()));
        }
    }

    Err(AuthError::Forbidden)
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
        (status = 200, description = "Register successful", body = String)
    )
)]
pub async fn register(session: Session<SessionRedisPool>, State(state): State<AppState>, Valid(Json(payload)): Valid<Json<RegisterRequest>>) -> Result<Json<String>, AuthError> {
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
        password_hash: Set(Some(password_hash)),
    };

    users::Entity::insert(user.clone())
        .exec(&state.db)
        .await
        .expect("insert user");


    session.set("user_id", user_id);
    Ok(Json("Register successful".to_string()))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/me",
    responses(
        (status = 200, description = "Current user info", body = entities::users::Model)
    )
)]
pub async fn me(State(state): State<AppState>) -> Json<entities::users::Model> {
    // Implementation for fetching current user info
    todo!()
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/logout",
    responses(
        (status = 200, description = "Logout successful", body = String)
    )
)]
pub async fn logout(State(state): State<AppState>) -> Json<String> {
    Json("Hello, world!".to_string())
}

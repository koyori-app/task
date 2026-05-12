use axum::{Json, extract::State};
use axum_valid::Valid;
use sea_orm::{ActiveValue::Set, EntityTrait};
use sea_orm::prelude::Uuid;
use serde::Deserialize;
use validator::Validate;

use crate::entities;
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
pub async fn login(State(state): State<AppState>) -> Json<String> {
    Json("Hello, world!".to_string())
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
pub async fn register(State(state): State<AppState>, Valid(Json(payload)): Valid<Json<RegisterRequest>>) -> Json<String> {
    let RegisterRequest {
        username,
        email,
        password,
    } = payload;

    let password_hash =
        bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("bcrypt hash");

    let user = users::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username),
        bio: Set(Some(String::new())),
        avatar_url: Set(None),
        email: Set(email),
        password_hash: Set(Some(password_hash)),
    };

    users::Entity::insert(user)
        .exec(&state.db)
        .await
        .expect("insert user");

    Json("Register successful".to_string())
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

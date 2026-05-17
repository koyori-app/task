use std::ops::Deref;

use axum::{extract::FromRequestParts, http::request::Parts};
use axum_session_redispool::SessionRedisPool;
use sea_orm::{EntityTrait, prelude::Uuid};

use crate::{AppState, entities::users, utils::auth::AuthError};

type Session = axum_session::Session<SessionRedisPool>;

async fn user_id_from_session(parts: &mut Parts, state: &AppState) -> Result<Uuid, AuthError> {
    let session = Session::from_request_parts(parts, state)
        .await
        .map_err(|_| AuthError::Internal(anyhow::anyhow!("session layer missing")))?;

    session
        .get::<Uuid>("user_id")
        .ok_or(AuthError::Unauthorized)
}

/// 認証済みユーザーの ID のみ（DB アクセスなし）
pub struct AuthUser {
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user_id = user_id_from_session(parts, state).await?;
        Ok(AuthUser { user_id })
    }
}

/// 認証済みユーザーの DB レコード（ハンドラで毎回取得する必要なし）
pub struct CurrentUser(pub users::Model);

impl Deref for CurrentUser {
    type Target = users::Model;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user_id = user_id_from_session(parts, state).await?;
        let user = users::Entity::find_by_id(user_id)
            .one(&state.db)
            .await?
            .ok_or(AuthError::Forbidden)?;
        Ok(CurrentUser(user))
    }
}

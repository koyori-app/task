use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use axum_session_redispool::SessionRedisPool;
use sea_orm::sqlx::types::uuid;
type Session = axum_session::Session<SessionRedisPool>;
pub struct AuthUser {
    pub user_id: uuid::Uuid,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
// 1. セッションをリクエストから取得
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Session layer missing"))?;

        // 2. セッションからユーザーIDを抽出
        if let Some(user_id) = session.get::<uuid::Uuid>("user_id") {
            Ok(AuthUser { user_id })
        } else {
            // 3. なければ401 (Unauthorized) を返す
            Err((StatusCode::UNAUTHORIZED, "Login required"))
        }
    }
}
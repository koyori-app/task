use sea_orm::prelude::Uuid;
use serde::Serialize;
use utoipa::ToSchema;

/// Public user profile — excludes password_hash, sessions_revoked_at, and other secrets.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "username")]
    pub username: String,
    #[schema(nullable)]
    pub bio: Option<String>,
    #[schema(nullable)]
    pub avatar_url: Option<String>,
    #[schema(value_type = String, format = "email")]
    pub email: String,
    pub email_verified: bool,
    pub is_admin: bool,
    pub is_suspended: bool,
    pub totp_enabled: bool,
}

/// 他リソースのレスポンスに埋め込む軽量なユーザー情報
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserSummary {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub username: String,
    #[schema(nullable)]
    pub avatar_url: Option<String>,
}

impl From<entity::users::Model> for UserSummary {
    fn from(model: entity::users::Model) -> Self {
        Self {
            id: model.id,
            username: model.username,
            avatar_url: model.avatar_url,
        }
    }
}

impl From<entity::users::Model> for UserResponse {
    fn from(model: entity::users::Model) -> Self {
        Self {
            id: model.id,
            username: model.username,
            bio: model.bio,
            avatar_url: model.avatar_url,
            email: model.email,
            email_verified: model.email_verified,
            is_admin: model.is_admin,
            is_suspended: model.is_suspended,
            totp_enabled: model.totp_enabled,
        }
    }
}

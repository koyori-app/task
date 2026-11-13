use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AdminCreateUserRequest {
    #[validate(length(min = 3))]
    pub username: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub email_verified: bool,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AdminUpdateUserRequest {
    pub is_admin: Option<bool>,
    pub is_suspended: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AdminPasswordResetRequest {
    #[validate(email)]
    pub send_to: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPasswordResetResponse {
    pub message: String,
}

/// 管理者に見せるパスキーの情報。公開鍵・credential_id・sign_count は含めない。
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPasskeyItem {
    pub id: sea_orm::prelude::Uuid,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// 管理者に見せる OAuth 連携の情報。トークン・provider_user_id は含めない。
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOAuthItem {
    pub provider: String,
    pub provider_login: Option<String>,
    pub provider_email: Option<String>,
    pub instance_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminAuthMethodsResponse {
    pub passkeys: Vec<AdminPasskeyItem>,
    pub oauth: Vec<AdminOAuthItem>,
}

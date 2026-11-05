use chrono::{DateTime, Utc};
use entity::{scopes::Scope, tenant_members::TenantRole};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateBotRequest {
    #[validate(length(min = 1, max = 100))]
    pub username: String,
    pub role: TenantRole,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBotRequest {
    pub is_suspended: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BotResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub account_kind: String,
    pub is_suspended: bool,
    pub role: TenantRole,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateBotTokenRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(min = 1))]
    pub scopes: Vec<Scope>,
    pub project_ids: Option<Vec<Uuid>>,
    /// 期限は必須。最長 365 日。
    pub expires_at: DateTime<Utc>,
}

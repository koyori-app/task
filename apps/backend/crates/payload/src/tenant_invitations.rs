use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::tenant_invitations;
use entity::tenant_members::TenantRole;

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct CreateTenantInvitationRequest {
    #[schema(value_type = String, format = "email")]
    #[validate(email)]
    pub email: String,
    pub role: TenantRole,
}

/// 保留中の招待。トークン（とそのハッシュ）は含めない。
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantInvitationResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    pub email: String,
    pub role: TenantRole,
    /// これを過ぎた招待は承諾できない。再送すると延びる
    #[schema(value_type = String, format = "date-time")]
    pub expires_at: DateTime<Utc>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTime<Utc>,
}

impl From<tenant_invitations::Model> for TenantInvitationResponse {
    fn from(model: tenant_invitations::Model) -> Self {
        Self {
            id: model.id,
            tenant_id: model.tenant_id,
            email: model.email,
            role: model.role,
            expires_at: model.expires_at.into(),
            created_at: model.created_at.into(),
        }
    }
}

/// 招待メールのリンクに載ったトークン。URL に残さないよう本文で受ける。
#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct InvitationTokenRequest {
    #[validate(length(min = 1, max = 256))]
    pub token: String,
}

/// 承諾画面に出す招待の中身。トークンを持つ人（招待メールの受け手）にだけ見せる。
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InvitationPreviewResponse {
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    pub tenant_name: String,
    /// 承諾後に開く URL（`/{display_id}`）に使う
    pub tenant_display_id: String,
    /// 承諾できるのはこのアドレスでログインしている利用者だけ
    pub email: String,
    pub role: TenantRole,
    pub invited_by: String,
    #[schema(value_type = String, format = "date-time")]
    pub expires_at: DateTime<Utc>,
}

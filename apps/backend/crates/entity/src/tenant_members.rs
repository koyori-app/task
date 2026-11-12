//! Tenant members entity — schema-first with hand-written DeriveActiveEnum.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(255))")]
pub enum TenantRole {
    #[sea_orm(string_value = "Admin")]
    Admin,
    #[sea_orm(string_value = "Member")]
    Member,
    #[sea_orm(string_value = "Viewer")]
    Viewer,
}

impl TenantRole {
    /// BOT に与えてよい役か。BOT は Member 相当までに縛る（Admin の BOT の PAT に
    /// `write:tenant` を与えると、PAT から人へ権限を委ねられるため）。要るようになれば緩める。
    pub fn allowed_for_bot(&self) -> bool {
        !matches!(self, Self::Admin)
    }
}

pub use super::_generated::tenant_members::*;

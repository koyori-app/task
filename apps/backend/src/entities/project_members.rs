//! Project members entity — schema-first with hand-written DeriveActiveEnum.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(255))")]
pub enum ProjectRole {
    #[sea_orm(string_value = "Admin")]
    Admin,
    #[sea_orm(string_value = "Member")]
    Member,
    #[sea_orm(string_value = "Viewer")]
    Viewer,
}

pub use super::_generated::project_members::*;

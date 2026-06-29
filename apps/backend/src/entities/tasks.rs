//! Tasks entity — schema-first with hand-written DeriveActiveEnum (OpenAPI core).
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum TaskPriority {
    #[sea_orm(string_value = "critical_fire")]
    CriticalFire,
    #[sea_orm(string_value = "critical")]
    Critical,
    #[sea_orm(string_value = "high")]
    High,
    #[sea_orm(string_value = "medium")]
    Medium,
    #[sea_orm(string_value = "low")]
    Low,
    #[sea_orm(string_value = "trivial")]
    Trivial,
}

pub use super::_generated::tasks::*;

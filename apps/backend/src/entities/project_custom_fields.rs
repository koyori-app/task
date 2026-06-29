//! Project custom fields entity — schema-first with hand-written DeriveActiveEnum.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum CustomFieldType {
    #[sea_orm(string_value = "text")]
    Text,
    #[sea_orm(string_value = "number")]
    Number,
    #[sea_orm(string_value = "select")]
    Select,
    #[sea_orm(string_value = "date")]
    Date,
    #[sea_orm(string_value = "url")]
    Url,
    #[sea_orm(string_value = "checkbox")]
    Checkbox,
}

pub use super::_generated::project_custom_fields::*;

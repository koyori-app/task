use sea_orm::FromJsonQueryResult;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};
use utoipa::ToSchema;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    EnumIter,
    ToSchema,
)]
#[strum(serialize_all = "snake_case")]
pub enum Scope {
    #[strum(serialize = "read:user")]
    #[serde(rename = "read:user")]
    ReadUser,
    #[strum(serialize = "write:user")]
    #[serde(rename = "write:user")]
    WriteUser,
    #[strum(serialize = "admin:all")]
    #[serde(rename = "admin:all")]
    AdminAll,
}

/// JSON カラム用の `Vec<Scope>` ラッパ（SeaORM エンティティ向け）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult, ToSchema)]
#[serde(transparent)]
pub struct ScopeList(pub Vec<Scope>);

impl From<Scope> for sea_orm::Value {
    fn from(source: Scope) -> Self {
        sea_orm::Value::String(Some(source.to_string()))
    }
}

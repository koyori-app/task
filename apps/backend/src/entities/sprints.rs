use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum SprintStatus {
    #[sea_orm(string_value = "planning")]
    Planning,
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "completed")]
    Completed,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "sprints")]
#[schema(as = crate::entities::sprints::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    pub name: String,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub goal: Option<String>,
    #[schema(value_type = String, example = "2026-06-01")]
    pub start_date: TimeDate,
    #[schema(value_type = String, example = "2026-06-14")]
    pub end_date: TimeDate,
    pub status: SprintStatus,
    #[schema(value_type = String, format = "uuid")]
    pub created_by: Uuid,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::projects::Entity",
        from = "Column::ProjectId",
        to = "super::projects::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Projects,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedBy",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Users,
}

impl Related<super::projects::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Projects.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

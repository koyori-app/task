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
        belongs_to = "super::project_statuses::Entity",
        from = "Column::StatusId",
        to = "super::project_statuses::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    ProjectStatuses,
    #[sea_orm(
        belongs_to = "super::milestones::Entity",
        from = "Column::MilestoneId",
        to = "super::milestones::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Milestones,
    #[sea_orm(
        belongs_to = "super::sprints::Entity",
        from = "Column::SprintId",
        to = "super::sprints::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Sprints,
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

impl Related<super::project_statuses::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProjectStatuses.def()
    }
}

impl Related<super::milestones::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Milestones.def()
    }
}

impl Related<super::sprints::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Sprints.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

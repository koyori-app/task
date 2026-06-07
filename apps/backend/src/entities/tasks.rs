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

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "tasks")]
#[schema(as = crate::entities::tasks::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    pub seq_id: i32,
    pub title: String,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub description: Option<String>,
    #[schema(value_type = String, format = "uuid")]
    pub status_id: Uuid,
    pub priority: TaskPriority,
    pub progress_pct: i16,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub parent_task_id: Option<Uuid>,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub milestone_id: Option<Uuid>,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub sprint_id: Option<Uuid>,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub soft_deadline: Option<DateTimeUtc>,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub hard_deadline: Option<DateTimeUtc>,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub estimated_minutes: Option<i32>,
    pub is_archived: bool,
    #[schema(value_type = String, format = "uuid")]
    pub created_by: Uuid,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: DateTimeUtc,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub completed_at: Option<DateTimeUtc>,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub deleted_at: Option<DateTimeUtc>,
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

impl ActiveModelBehavior for ActiveModel {}

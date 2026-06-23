use chrono::NaiveDate;
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::entities::sprints;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateSprintRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub goal: Option<String>,
    #[schema(value_type = String, example = "2026-06-01")]
    pub start_date: NaiveDate,
    #[schema(value_type = String, example = "2026-06-14")]
    pub end_date: NaiveDate,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateSprintRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub goal: Option<String>,
    #[serde(default)]
    pub clear_goal: bool,
    #[schema(value_type = Option<String>, example = "2026-06-01")]
    pub start_date: Option<NaiveDate>,
    #[schema(value_type = Option<String>, example = "2026-06-14")]
    pub end_date: Option<NaiveDate>,
}

#[derive(Deserialize, ToSchema, IntoParams)]
pub struct ListSprintsQuery {
    pub status: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CompleteSprintRequest {
    #[schema(value_type = Option<String>, format = "uuid")]
    pub move_incomplete_to_sprint_id: Option<Uuid>,
    #[serde(default)]
    pub move_incomplete_to_backlog: bool,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct AssignTasksRequest {
    #[validate(length(min = 1))]
    pub task_ids: Vec<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub struct SprintTaskCounts {
    pub total: usize,
    pub done: usize,
    pub in_progress: usize,
}

#[derive(Serialize, ToSchema)]
pub struct BurndownPoint {
    #[schema(value_type = String, example = "2026-06-01")]
    pub date: time::Date,
    pub ideal_remaining: i32,
    pub actual_remaining: usize,
}

#[derive(Serialize, ToSchema)]
pub struct SprintDetail {
    #[serde(flatten)]
    pub sprint: sprints::Model,
    pub task_counts: SprintTaskCounts,
    pub burndown: Vec<BurndownPoint>,
}

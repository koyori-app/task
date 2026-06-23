use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::entities::milestones;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateMilestoneRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub description: Option<String>,
    pub due_date: time::Date,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateMilestoneRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub clear_description: bool,
    pub due_date: Option<time::Date>,
}

#[derive(Serialize, ToSchema)]
pub struct MilestoneDetail {
    #[serde(flatten)]
    pub milestone: milestones::Model,
    pub progress_pct: u32,
    pub task_counts: TaskCounts,
}

#[derive(Serialize, ToSchema)]
pub struct TaskCounts {
    pub total: usize,
    pub done: usize,
}

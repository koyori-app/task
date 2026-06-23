use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::entities::tasks;
use crate::utils::custom_fields::{CustomFieldValueInput, TaskCustomFieldValueResponse};

#[derive(Deserialize, ToSchema)]
pub struct AssigneeInput {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateTaskRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: String,
    pub description: Option<String>,
    #[schema(value_type = String, format = "uuid")]
    pub status_id: Uuid,
    pub priority: Option<tasks::TaskPriority>,
    #[validate(range(min = 0, max = 100))]
    pub progress_pct: Option<i16>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_task_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub milestone_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub sprint_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub soft_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub hard_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[validate(range(min = 1))]
    pub estimated_minutes: Option<i32>,
    #[serde(default)]
    pub assignees: Vec<AssigneeInput>,
    #[serde(default)]
    pub label_ids: Vec<Uuid>,
    #[serde(default)]
    pub custom_field_values: Vec<CustomFieldValueInput>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateTaskRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub clear_description: bool,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub status_id: Option<Uuid>,
    pub priority: Option<tasks::TaskPriority>,
    #[validate(range(min = 0, max = 100))]
    pub progress_pct: Option<i16>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_task_id: Option<Uuid>,
    #[serde(default)]
    pub clear_parent_task_id: bool,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub milestone_id: Option<Uuid>,
    #[serde(default)]
    pub clear_milestone_id: bool,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub sprint_id: Option<Uuid>,
    #[serde(default)]
    pub clear_sprint_id: bool,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub soft_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub clear_soft_deadline: bool,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub hard_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub clear_hard_deadline: bool,
    #[validate(range(min = 1))]
    pub estimated_minutes: Option<i32>,
    #[serde(default)]
    pub clear_estimated_minutes: bool,
    pub is_archived: Option<bool>,
    pub custom_field_values: Option<Vec<CustomFieldValueInput>>,
}

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct ListTasksQuery {
    pub status_id: Option<Uuid>,
    pub priority: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub milestone_id: Option<Uuid>,
    pub sprint_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    #[serde(default)]
    pub is_archived: bool,
    pub sort: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 {
    50
}

#[derive(Serialize, ToSchema)]
pub struct TaskListResponse {
    pub tasks: Vec<tasks::Model>,
    pub total: u64,
}

#[derive(Serialize, ToSchema)]
pub struct TaskDetailResponse {
    #[serde(flatten)]
    pub task: tasks::Model,
    pub custom_field_values: Vec<TaskCustomFieldValueResponse>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct AddAssigneeRequest {
    pub user_id: Uuid,
    #[validate(length(min = 1))]
    pub role: String,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateAssigneeRequest {
    #[validate(length(min = 1))]
    pub role: String,
}

#[derive(Serialize, ToSchema)]
pub struct RelationEntry {
    pub relation_id: Uuid,
    #[serde(flatten)]
    pub task: tasks::Model,
}

#[derive(Serialize, ToSchema)]
pub struct TaskRelationsResponse {
    pub subtasks: Vec<tasks::Model>,
    pub blocks: Vec<RelationEntry>,
    pub blocked_by: Vec<RelationEntry>,
}

#[derive(Deserialize, ToSchema)]
pub struct AddRelationRequest {
    #[serde(rename = "type")]
    pub relation_type: String,
    pub target_task_id: Uuid,
}

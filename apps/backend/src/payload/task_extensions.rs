use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::entities::project_task_views;

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct SearchTasksQuery {
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_search_limit() -> u64 {
    20
}

#[derive(Serialize, ToSchema)]
pub struct SearchTaskHit {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub seq_id: i32,
    pub title: String,
    pub highlight: String,
    pub score: f32,
}

#[derive(Serialize, ToSchema)]
pub struct SearchTasksResponse {
    pub tasks: Vec<SearchTaskHit>,
    pub total: u64,
}

#[derive(Deserialize, ToSchema)]
pub struct BulkUpdateFields {
    #[schema(value_type = Option<String>, format = "uuid")]
    pub status_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub assignee_id: Option<Uuid>,
    /// 既存ラベルに追加する ID 一覧（上書きではない）。
    pub add_label_ids: Option<Vec<Uuid>>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub sprint_id: Option<Uuid>,
    #[serde(default)]
    pub clear_sprint_id: bool,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct BulkUpdateRequest {
    #[validate(length(min = 1))]
    pub task_ids: Vec<Uuid>,
    pub update: BulkUpdateFields,
}

#[derive(Serialize, ToSchema)]
pub struct BulkUpdateResponse {
    pub updated: u32,
    pub failed: Vec<BulkFailure>,
}

#[derive(Serialize, ToSchema)]
pub struct BulkFailure {
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    pub reason: String,
}

#[derive(Serialize, ToSchema)]
pub struct TaskViewListResponse {
    pub views: Vec<project_task_views::Model>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateTaskViewRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[serde(default)]
    pub is_shared: bool,
    #[serde(default)]
    pub filters: serde_json::Value,
    #[serde(default)]
    pub sort: serde_json::Value,
    #[serde(default = "default_view_type")]
    #[validate(custom(function = "validate_view_type"))]
    pub view_type: String,
}

fn default_view_type() -> String {
    "list".into()
}

fn validate_view_type(view_type: &str) -> Result<(), validator::ValidationError> {
    match view_type {
        "board" | "list" | "table" => Ok(()),
        _ => Err(validator::ValidationError::new("view_type")),
    }
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateTaskViewRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub is_shared: Option<bool>,
    pub filters: Option<serde_json::Value>,
    pub sort: Option<serde_json::Value>,
    #[validate(custom(function = "validate_view_type"))]
    pub view_type: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct TaskAttachmentResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub drive_file_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size: i64,
    pub url: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct TaskAttachmentListResponse {
    pub attachments: Vec<TaskAttachmentResponse>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct AttachFileRequest {
    #[schema(value_type = String, format = "uuid")]
    pub drive_file_id: Uuid,
}

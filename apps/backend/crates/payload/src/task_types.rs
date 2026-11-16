use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::project_task_types;

/// タスク種別の定義。プロジェクトごとに持ち、1 つのタスクに 0 か 1 つ付く
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskTypeResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
    /// 並び順（昇順）
    pub position: i16,
}

impl From<project_task_types::Model> for TaskTypeResponse {
    fn from(model: project_task_types::Model) -> Self {
        Self {
            id: model.id,
            project_id: model.project_id,
            name: model.name,
            color: model.color,
            position: model.position,
        }
    }
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateTaskTypeRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(regex(path = "common::validation::COLOR_REGEX"))]
    pub color: String,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateTaskTypeRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    #[validate(regex(path = "common::validation::COLOR_REGEX"))]
    pub color: Option<String>,
}

/// 並び順の一括更新。プロジェクトの全種別の ID を、並べたい順に過不足なく渡す
#[derive(Deserialize, ToSchema)]
pub struct ReorderTaskTypesRequest {
    #[schema(value_type = Vec<String>, format = "uuid")]
    pub ids: Vec<Uuid>,
}

use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::project_members::{self, ProjectRole};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProjectMemberResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub role: ProjectRole,
}

impl From<project_members::Model> for ProjectMemberResponse {
    fn from(model: project_members::Model) -> Self {
        Self {
            id: model.id,
            project_id: model.project_id,
            user_id: model.user_id,
            role: model.role,
        }
    }
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct AddMemberRequest {
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub role: ProjectRole,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct UpdateMemberRequest {
    pub role: ProjectRole,
}

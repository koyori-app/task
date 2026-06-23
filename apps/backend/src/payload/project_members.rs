use sea_orm::prelude::Uuid;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

use crate::entities::project_members::ProjectRole;

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

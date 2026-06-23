use sea_orm::prelude::Uuid;
use serde::Serialize;
use utoipa::ToSchema;

use crate::entities::{projects, tenants};

#[derive(Serialize, ToSchema)]
pub struct AdminTenantListResponse {
    pub tenants: Vec<tenants::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminProjectListResponse {
    pub projects: Vec<projects::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminTaskRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
}

#[derive(Serialize, ToSchema)]
pub struct AdminTaskListResponse {
    pub tasks: Vec<AdminTaskRow>,
}

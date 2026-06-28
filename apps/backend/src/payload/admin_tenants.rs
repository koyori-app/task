use sea_orm::prelude::Uuid;
use serde::Serialize;
use utoipa::ToSchema;

use crate::payload::{projects::ProjectResponse, tenants::TenantResponse};

#[derive(Serialize, ToSchema)]
pub struct AdminTenantListResponse {
    pub tenants: Vec<TenantResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminProjectListResponse {
    pub projects: Vec<ProjectResponse>,
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

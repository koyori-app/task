use sea_orm::prelude::Uuid;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateStatusRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(regex(path = "crate::utils::validation::COLOR_REGEX"))]
    pub color: String,
    pub position: i16,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_done_state: bool,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateStatusRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    #[validate(regex(path = "crate::utils::validation::COLOR_REGEX"))]
    pub color: Option<String>,
    pub position: Option<i16>,
    pub is_default: Option<bool>,
    pub is_done_state: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct ReorderRequest {
    pub ids: Vec<Uuid>,
}

#[derive(Deserialize, ToSchema)]
pub struct DeleteStatusQuery {
    pub migrate_to_status_id: Option<Uuid>,
}

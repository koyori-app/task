use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use validator::Validate;

use crate::entities::project_custom_fields;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateCustomFieldRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub field_type: project_custom_fields::CustomFieldType,
    pub options: Option<Value>,
    #[serde(default)]
    pub is_required: bool,
    pub position: Option<i16>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateCustomFieldRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub options: Option<Value>,
    pub is_required: Option<bool>,
    pub position: Option<i16>,
}

#[derive(Serialize, ToSchema)]
pub struct CustomFieldListResponse {
    pub fields: Vec<project_custom_fields::Model>,
}

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct CreateTenantRequest {
    #[validate(length(min = 1))]
    pub display_id: String,
    #[validate(length(min = 1))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_url: String,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct UpdateTenantRequest {
    #[validate(length(min = 1))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
}

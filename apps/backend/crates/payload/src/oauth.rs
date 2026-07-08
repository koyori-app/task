use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
    #[serde(default)]
    pub redirect_after: Option<String>,
    #[serde(default)]
    pub instance_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DisconnectQuery {
    #[serde(default)]
    pub instance_url: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct OAuthConnectionItem {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_url: Option<String>,
    pub connected_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct OAuthConnectionsResponse {
    pub connections: Vec<OAuthConnectionItem>,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct SetPasswordRequest {
    #[schema(value_type = String, format = "password")]
    #[validate(length(min = 8))]
    pub password: String,
}

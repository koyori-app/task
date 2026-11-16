use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

#[derive(Validate, Deserialize, ToSchema)]
pub struct ReauthPasswordRequest {
    #[validate(length(min = 1, max = 1024))]
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ReauthPasskeyFinishRequest {
    #[schema(value_type = Object)]
    pub credential: serde_json::Value,
}

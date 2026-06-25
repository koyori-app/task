use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    #[validate(length(min = 1))]
    pub name: String,
    #[schema(value_type = String, format = "uuid", nullable)]
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFolderRequest {
    pub name: Option<String>,
    /// `null` でルートへ移動。フィールド省略時は変更なし。
    #[schema(value_type = String, format = "uuid", nullable)]
    pub parent_id: Option<Option<Uuid>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateShareRequest {
    /// `user` or `public_link`
    #[serde(rename = "type")]
    pub share_type: String,
    #[schema(value_type = String, format = "uuid", nullable)]
    pub user_id: Option<Uuid>,
    pub permission: String,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicFolderResponse {
    pub name: String,
    pub created_by_name: String,
    pub file_count: u64,
}

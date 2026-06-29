use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::Serialize;
use utoipa::ToSchema;

use crate::entities::drive_folder_shares::{self, SharePermission};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DriveFolderShareResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub folder_id: Uuid,
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub shared_with_user_id: Option<Uuid>,
    #[schema(nullable)]
    pub share_token: Option<String>,
    pub permission: SharePermission,
    #[schema(value_type = String, format = "uuid")]
    pub created_by: Uuid,
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub expires_at: Option<DateTime<Utc>>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTime<Utc>,
}

impl From<drive_folder_shares::Model> for DriveFolderShareResponse {
    fn from(model: drive_folder_shares::Model) -> Self {
        Self {
            id: model.id,
            folder_id: model.folder_id,
            shared_with_user_id: model.shared_with_user_id,
            share_token: model.share_token,
            permission: model.permission,
            created_by: model.created_by,
            expires_at: model.expires_at.map(|dt| dt.with_timezone(&Utc)),
            created_at: model.created_at.with_timezone(&Utc),
        }
    }
}

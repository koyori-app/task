use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Serialize, ToSchema)]
pub struct CommentUser {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct CommentReply {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub user: CommentUser,
    #[schema(nullable)]
    pub body: Option<String>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_deleted: bool,
}

#[derive(Serialize, ToSchema)]
pub struct CommentThread {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub user: CommentUser,
    #[schema(nullable)]
    pub body: Option<String>,
    pub replies: Vec<CommentReply>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_deleted: bool,
}

#[derive(Serialize, ToSchema)]
pub struct CommentListResponse {
    pub comments: Vec<CommentThread>,
}

#[derive(Serialize, ToSchema)]
pub struct ActivityUser {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct ActivityItem {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub event_type: String,
    #[schema(nullable)]
    pub user: Option<ActivityUser>,
    #[schema(value_type = serde_json::Value)]
    pub payload: serde_json::Value,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct ActivityListResponse {
    pub activities: Vec<ActivityItem>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateCommentRequest {
    #[validate(length(min = 1))]
    pub body: String,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_comment_id: Option<Uuid>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateCommentRequest {
    #[validate(length(min = 1))]
    pub body: String,
}

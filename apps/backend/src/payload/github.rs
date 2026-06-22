use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct GithubCallbackQuery {
    pub installation_id: i64,
    pub state: String,
    /// GitHub が送る操作種別。"request" はオーナー承認待ちであり連携未完了。
    #[serde(default)]
    pub setup_action: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubIntegrationResponse {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String, format = "date-time", nullable)]
    pub connected_at: Option<DateTimeWithTimeZone>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubInstallUrlResponse {
    pub url: String,
}

use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GithubCallbackQuery {
    pub installation_id: i64,
    pub state: String,
    /// GitHub が送る操作種別。"request" はオーナー承認待ちであり連携未完了。
    #[serde(default)]
    pub setup_action: Option<String>,
    /// インストール時のユーザー認可で GitHub が付ける認可コード。
    /// これを交換して得たユーザーアクセストークンで、installation の所有者を確認する。
    #[serde(default)]
    pub code: Option<String>,
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
    pub connected_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubInstallUrlResponse {
    pub url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubRepositoryItem {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubRepositoriesResponse {
    pub repositories: Vec<GithubRepositoryItem>,
}

/// 同じテナントで利用中の GitHub インストール（再利用候補）。
#[derive(Debug, Serialize, ToSchema)]
pub struct GithubReusableInstallationItem {
    /// 代表となる既存連携行の ID。`POST /github/reuse` に渡す。
    pub source_integration_id: Uuid,
    /// 表示名（既存連携行のリポジトリオーナー）。
    pub account_login: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubReusableInstallationsResponse {
    pub installations: Vec<GithubReusableInstallationItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GithubReuseRequest {
    pub source_integration_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubReuseResponse {
    pub select_token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GithubConnectRequest {
    pub select_token: String,
    pub repo_owner: String,
    pub repo_name: String,
}

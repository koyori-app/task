use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GithubCallbackQuery {
    /// インストール画面からの戻りで GitHub が付ける。ユーザー認可だけの戻りには付かない。
    #[serde(default)]
    pub installation_id: Option<i64>,
    pub state: String,
    /// ユーザー認可を拒否したときに GitHub が付ける理由（`access_denied` など）。
    #[serde(default)]
    pub error: Option<String>,
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

/// GitHub のユーザー認可で確かめた、利用者が見えるインストール（連携の候補）。
#[derive(Debug, Serialize, ToSchema)]
pub struct GithubAuthorizedInstallationItem {
    /// 候補の ID。`POST /github/authorized-installations/select` に渡す。
    pub candidate_id: Uuid,
    /// 表示名（インストール先のアカウント・組織）。
    pub account_login: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubAuthorizedInstallationsResponse {
    pub installations: Vec<GithubAuthorizedInstallationItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GithubAuthorizedSelectRequest {
    pub authorize_token: String,
    pub candidate_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GithubConnectRequest {
    pub select_token: String,
    pub repo_owner: String,
    pub repo_name: String,
}

/// `GET /v1/github/snippet` のクエリ。GitHub のパーマリンク
/// （`/{owner}/{repo}/blob/{sha}/{path}#L{start}-L{end}`）を分解した値。
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GithubSnippetQuery {
    pub owner: String,
    pub repo: String,
    /// 40 桁のコミット SHA（ブランチ名・短縮 SHA は受けない）
    pub sha: String,
    /// リポジトリ直下からのファイルのパス
    pub path: String,
    /// 抜粋の最初の行（1 始まり）
    pub start: u32,
    /// 抜粋の最後の行。省くと `start` の 1 行だけ
    #[serde(default)]
    pub end: Option<u32>,
}

/// 連携リポジトリのファイルから切り出した行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GithubSnippetResponse {
    pub repo_owner: String,
    pub repo_name: String,
    pub sha: String,
    pub path: String,
    pub start_line: u32,
    /// 実際に返した最後の行（ファイルの行数と上限で丸める）
    pub end_line: u32,
    pub lines: Vec<String>,
    /// 上限で打ち切ったときに true
    pub truncated: bool,
}

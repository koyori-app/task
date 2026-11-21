//! MCP の OAuth の同意画面と、連携中のアプリの管理の DTO。

use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::scopes::{Scope, ScopeList};

/// クライアントが認可エンドポイントへ付けてきた要求（query の中身）。
///
/// `response_type`・`code_challenge`・`code_challenge_method` は、欠けていても受け取り、
/// 同意の API が項目ごとのエラーで断る。
#[derive(Validate, Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthorizationParams {
    /// クライアントの識別子（Client ID Metadata Document の URL）
    #[validate(length(min = 1, max = 2048))]
    #[schema(max_length = 2048)]
    pub client_id: String,
    #[validate(length(min = 1, max = 2048))]
    #[schema(max_length = 2048)]
    pub redirect_uri: String,
    #[serde(default)]
    #[validate(length(max = 64))]
    #[schema(max_length = 64)]
    pub response_type: String,
    #[serde(default)]
    #[validate(length(max = 128))]
    #[schema(max_length = 128)]
    pub code_challenge: String,
    #[serde(default)]
    #[validate(length(max = 64))]
    #[schema(max_length = 64)]
    pub code_challenge_method: String,
    #[validate(length(max = 2048))]
    #[schema(max_length = 2048, nullable)]
    pub state: Option<String>,
    /// 空白区切りの scope。省略すると、渡せる scope の全部を求めたものとして読む
    #[validate(length(max = 1024))]
    #[schema(max_length = 1024, nullable)]
    pub scope: Option<String>,
    /// トークンの宛先（MCP の入口の URL）
    #[validate(length(max = 2048))]
    #[schema(max_length = 2048, nullable)]
    pub resource: Option<String>,
}

/// 同意の承認。要求の中身に、利用者が選んだテナントと scope を足す。
#[derive(Validate, Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApproveAuthorizationRequest {
    #[serde(flatten)]
    #[validate(nested)]
    pub params: AuthorizationParams,
    /// 接続を使えるテナント。選んだすべてで、所属と 2FA を確かめる
    #[validate(length(min = 1, max = 20))]
    #[schema(value_type = Vec<String>, format = "uuid", min_items = 1, max_items = 20)]
    pub tenant_ids: Vec<Uuid>,
    /// 許可する scope。要求された scope の部分集合
    #[validate(length(min = 1))]
    #[schema(min_items = 1)]
    pub scopes: Vec<Scope>,
}

/// 同意画面に出す中身。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthorizationPreviewResponse {
    pub client_id: String,
    /// クライアントが名乗る名前。自己申告なので、画面は `client_host` を主に出す
    pub client_name: String,
    /// `client_id` の URL の host
    pub client_host: String,
    /// 承認のあとで戻る先の host。ポートが明示されていれば `host:port`
    pub redirect_host: String,
    /// 戻る先が利用者の端末（loopback の http）か
    pub redirect_is_loopback: bool,
    /// 要求された scope のうち、渡せるもの
    pub scopes: Vec<Scope>,
}

/// 画面が遷移する先（クライアントのコールバック）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthorizationRedirectResponse {
    pub redirect_to: String,
}

/// 連携中のアプリ（OAuth の許可）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OAuthGrantResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub client_id: String,
    pub client_name: String,
    /// 接続を使えるテナント（昇順）
    #[schema(value_type = Vec<String>, format = "uuid")]
    pub tenant_ids: Vec<Uuid>,
    pub scopes: ScopeList,
    /// この時刻まで使わなければ切れる。使うたびに延びる
    #[schema(value_type = String, format = "date-time")]
    pub expires_at: DateTime<Utc>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub last_used_at: Option<DateTime<Utc>>,
}

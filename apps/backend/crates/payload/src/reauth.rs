use serde::{Deserialize, Serialize};
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

/// 再認証の要否と、使える手段。
#[derive(Serialize, ToSchema)]
pub struct ReauthStatusResponse {
    /// 直近の本人確認が有効期間の内か。内なら、再認証を求める操作をそのまま撃てる
    pub fresh: bool,
    /// 有効期間の切れる時刻（RFC 3339）。`fresh` でなければ null
    pub expires_at: Option<String>,
    /// 切れるまでの残りの秒数（切り捨て）。`fresh` でなければ null。
    /// 画面は端末の時計でなくこの値で判じる（`expires_at` は端末の時計がずれると読み違える）
    pub remaining_secs: Option<u64>,
    /// 有効期間の長さ（秒）。本人確認をした後、この間は再認証を求めない
    pub window_secs: u64,
    pub methods: ReauthMethods,
}

/// 再認証に使える手段。
#[derive(Serialize, ToSchema)]
pub struct ReauthMethods {
    /// パスワードを持つか（`POST /v1/auth/reauth/password`）
    pub password: bool,
    /// パスキーを持つか（`POST /v1/auth/reauth/passkey/start`）
    pub passkey: bool,
    /// 再認証に使える連携（`GET /v1/auth/oauth/{provider}?reauth=true`）。ログインに使えぬ連携と、
    /// 設定から外したプロバイダーの連携は含まない
    pub oauth: Vec<ReauthOAuthMethod>,
}

/// 再認証に使える連携。
#[derive(Serialize, ToSchema)]
pub struct ReauthOAuthMethod {
    /// 開始用の slug（`GET /v1/auth/oauth/{provider}` の provider）
    pub provider: String,
    /// 連携一覧（`GET /v1/auth/oauth/connections`）の provider と同じ、DB の provider キー
    pub connection_provider: String,
    /// GitLab self-hosted の instance URL。開始に `instance_url` として渡す
    pub instance_url: Option<String>,
}

use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::device_tokens;

/// Desktop に渡す認可コードの発行要求（Web の承認画面から送る）。
#[derive(Validate, Debug, Serialize, Deserialize, ToSchema)]
pub struct DesktopAuthCodeRequest {
    /// S256 の code_challenge（base64url、43〜128 文字）
    #[validate(regex(path = "common::validation::PKCE_CHALLENGE_REGEX"))]
    pub code_challenge: String,
    /// 端末名（クライアントの申告。表示用）
    #[validate(length(min = 1, max = 100))]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DesktopAuthCodeResponse {
    /// 認可コード（5 分・一度きり）
    pub code: String,
}

/// 認可コードと code_verifier を Device Token に交換する要求（Desktop から送る）。
#[derive(Validate, Debug, Serialize, Deserialize, ToSchema)]
pub struct DesktopAuthTokenRequest {
    #[validate(length(min = 1, max = 256))]
    pub code: String,
    /// RFC 7636 の code_verifier（43〜128 文字）
    #[validate(length(min = 43, max = 128))]
    pub code_verifier: String,
}

/// 発行した Device Token（平文はこの応答でのみ返す）。
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DesktopAuthTokenResponse {
    pub token: String,
    #[schema(value_type = String, format = "date-time")]
    pub expires_at: DateTime<Utc>,
    #[schema(value_type = String, format = "uuid")]
    pub device_id: Uuid,
}

/// 端末（Device Token）のメタデータ。平文・ハッシュは含まない。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceToken {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub token_last_four: String,
    #[schema(value_type = String, format = "date-time")]
    pub expires_at: DateTime<Utc>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub last_used_at: Option<DateTime<Utc>>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTime<Utc>,
}

impl From<device_tokens::Model> for DeviceToken {
    fn from(model: device_tokens::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            token_last_four: model.token_last_four,
            expires_at: model.expires_at.with_timezone(&Utc),
            last_used_at: model.last_used_at.map(|dt| dt.with_timezone(&Utc)),
            created_at: model.created_at.with_timezone(&Utc),
        }
    }
}

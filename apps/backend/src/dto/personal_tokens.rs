use sea_orm::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;
use utoipa::ToSchema;

use crate::entities::{personal_tokens, scopes::ScopeList};

/// PAT のメタデータ（平文トークン・ハッシュは含まない）
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonalTokenResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub token_last_four: String,
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    #[schema(value_type = Vec<String>, format = "uuid", nullable)]
    pub project_ids: Option<Vec<Uuid>>,
    pub scopes: ScopeList,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<DateTimeWithTimeZone>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub last_used_at: Option<DateTimeWithTimeZone>,
    pub revoked: bool,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
}

impl From<personal_tokens::Model> for PersonalTokenResponse {
    fn from(model: personal_tokens::Model) -> Self {
        let project_ids = model
            .allowed_project_ids
            .as_ref()
            .and_then(|v| personal_tokens::parse_allowed_project_ids(v).ok().flatten());

        Self {
            id: model.id,
            name: model.name,
            token_last_four: model.token_last_four,
            tenant_id: model.tenant_id,
            project_ids,
            expires_at: model.expires_at,
            last_used_at: model.last_used_at,
            revoked: model.revoked,
            user_id: model.user_id,
            scopes: model.scopes,
        }
    }
}

/// PAT 作成時のレスポンス（平文トークンはこの応答でのみ返却）
#[derive(Clone, Serialize, ToSchema)]
pub struct CreatePersonalTokenResponse {
    pub token: String,
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub token_last_four: String,
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    #[schema(value_type = Vec<String>, format = "uuid", nullable)]
    pub project_ids: Option<Vec<Uuid>>,
    pub scopes: ScopeList,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<DateTimeWithTimeZone>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub last_used_at: Option<DateTimeWithTimeZone>,
    pub revoked: bool,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
}

impl CreatePersonalTokenResponse {
    pub fn new(token: String, model: personal_tokens::Model) -> Self {
        let metadata = PersonalTokenResponse::from(model);
        Self {
            token,
            id: metadata.id,
            name: metadata.name,
            token_last_four: metadata.token_last_four,
            tenant_id: metadata.tenant_id,
            project_ids: metadata.project_ids,
            expires_at: metadata.expires_at,
            last_used_at: metadata.last_used_at,
            revoked: metadata.revoked,
            user_id: metadata.user_id,
            scopes: metadata.scopes,
        }
    }
}

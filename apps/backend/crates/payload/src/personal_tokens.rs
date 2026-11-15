use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::or_unknown::OrUnknown;
use entity::{personal_tokens, scopes::Scope, scopes::ScopeList};

/// 1 本の PAT を結び付けられるテナント数の上限。作成時にテナントごとの権限判定が走る。
pub const MAX_TOKEN_TENANTS: usize = 20;

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct CreatePersonalTokenRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    /// トークンを使えるテナント。選んだすべてで、所属・2FA・Admin を確かめる
    #[validate(length(min = 1, max = 20))]
    #[schema(value_type = Vec<String>, format = "uuid", min_items = 1, max_items = 20)]
    pub tenant_ids: Vec<Uuid>,
    #[schema(value_type = Vec<String>, format = "uuid", nullable)]
    pub project_ids: Option<Vec<Uuid>>,
    #[validate(length(min = 1))]
    #[schema(min_items = 1)]
    pub scopes: Vec<Scope>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct RevokeAllPersonalTokensRequest {
    #[schema(value_type = String, format = "uuid")]
    pub confirm_tenant_id: Uuid,
}

/// PAT のメタデータ（平文トークン・ハッシュは含まない）
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonalTokenResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub token_last_four: String,
    /// トークンが結び付けられたテナント（昇順）
    #[schema(value_type = Vec<String>, format = "uuid")]
    pub tenant_ids: Vec<Uuid>,
    #[schema(value_type = Vec<String>, format = "uuid", nullable)]
    pub project_ids: Option<Vec<Uuid>>,
    pub scopes: ScopeList,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<DateTime<Utc>>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
}

/// PAT 自身の識別情報。アカウントの機微な状態は含めない。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PersonalTokenIdentityResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub username: String,
    #[serde(default = "crate::users::default_account_kind")]
    #[schema(required = false)]
    pub account_kind: String,
    /// この PAT が結び付けられたテナント（昇順）。鍵だけ渡された自動化クライアントが
    /// 行き先（/v1/tenants/{id}/... の {id}）を自力で知るための欄。旧サーバーの応答には
    /// 無いので、読む側は `bound_tenant_ids()` を使う
    #[serde(default)]
    #[schema(value_type = Vec<String>, format = "uuid", required = false)]
    pub tenant_ids: Vec<Uuid>,
    /// 束縛がちょうど 1 件のときだけ入る、旧 CLI 向けの欄。複数のときに入れると、旧 CLI が
    /// その 1 つへ黙って書き込む。入れなければ旧 CLI は応答を読めず「CLI を更新する」案内で止まる
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String, format = "uuid", required = false)]
    pub tenant_id: Option<Uuid>,
    #[schema(inline)]
    pub scopes: Vec<OrUnknown<Scope>>,
    #[schema(value_type = Vec<String>, format = "uuid", nullable)]
    pub allowed_project_ids: Option<Vec<Uuid>>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<DateTime<Utc>>,
}

impl PersonalTokenIdentityResponse {
    /// 結び付けられたテナント。旧サーバーの応答（`tenant_id` だけ）も 1 件として読む。
    pub fn bound_tenant_ids(&self) -> Vec<Uuid> {
        if self.tenant_ids.is_empty() {
            self.tenant_id.into_iter().collect()
        } else {
            self.tenant_ids.clone()
        }
    }
}

impl PersonalTokenResponse {
    /// `tenant_ids` は `personal_token_tenants` から引いた束縛（昇順）。
    pub fn from_parts(
        model: personal_tokens::Model,
        tenant_ids: Vec<Uuid>,
    ) -> Result<Self, serde_json::Error> {
        let project_ids = match model.allowed_project_ids.as_ref() {
            None => None,
            Some(v) => personal_tokens::parse_allowed_project_ids(v)?,
        };

        Ok(Self {
            id: model.id,
            name: model.name,
            token_last_four: model.token_last_four,
            tenant_ids,
            project_ids,
            expires_at: model.expires_at.map(|dt| dt.with_timezone(&Utc)),
            last_used_at: model.last_used_at.map(|dt| dt.with_timezone(&Utc)),
            revoked: model.revoked,
            user_id: model.user_id,
            scopes: model.scopes,
        })
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
    /// トークンが結び付けられたテナント（昇順）
    #[schema(value_type = Vec<String>, format = "uuid")]
    pub tenant_ids: Vec<Uuid>,
    #[schema(value_type = Vec<String>, format = "uuid", nullable)]
    pub project_ids: Option<Vec<Uuid>>,
    pub scopes: ScopeList,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<DateTime<Utc>>,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
}

impl CreatePersonalTokenResponse {
    pub fn new(
        token: String,
        model: personal_tokens::Model,
        tenant_ids: Vec<Uuid>,
    ) -> Result<Self, serde_json::Error> {
        let metadata = PersonalTokenResponse::from_parts(model, tenant_ids)?;
        Ok(Self {
            token,
            id: metadata.id,
            name: metadata.name,
            token_last_four: metadata.token_last_four,
            tenant_ids: metadata.tenant_ids,
            project_ids: metadata.project_ids,
            expires_at: metadata.expires_at,
            last_used_at: metadata.last_used_at,
            revoked: metadata.revoked,
            user_id: metadata.user_id,
            scopes: metadata.scopes,
        })
    }
}

#[cfg(test)]
mod legacy_identity_tests {
    #[test]
    fn legacy_pat_identity_defaults_to_human() {
        let identity: super::PersonalTokenIdentityResponse = serde_json::from_value(serde_json::json!({
            "id": sea_orm::prelude::Uuid::new_v4(), "name": "legacy", "user_id": sea_orm::prelude::Uuid::new_v4(),
            "username": "legacy", "tenant_id": sea_orm::prelude::Uuid::new_v4(), "scopes": ["read:task"],
            "allowed_project_ids": null, "expires_at": null
        })).expect("legacy PAT identity");
        assert_eq!(identity.account_kind, "human");
        // 旧サーバーは tenant_id だけを返す。1 件として読める
        assert_eq!(
            identity.bound_tenant_ids(),
            vec![identity.tenant_id.expect("legacy tenant_id")]
        );
    }

    #[test]
    fn identity_prefers_tenant_ids_and_omits_tenant_id_for_several() {
        let (a, b) = (
            sea_orm::prelude::Uuid::new_v4(),
            sea_orm::prelude::Uuid::new_v4(),
        );
        let identity: super::PersonalTokenIdentityResponse = serde_json::from_value(serde_json::json!({
            "id": sea_orm::prelude::Uuid::new_v4(), "name": "n", "user_id": sea_orm::prelude::Uuid::new_v4(),
            "username": "u", "tenant_ids": [a, b], "scopes": ["read:task"],
            "allowed_project_ids": null, "expires_at": null
        })).expect("multi-tenant PAT identity");
        assert_eq!(identity.bound_tenant_ids(), vec![a, b]);
        let json = serde_json::to_value(&identity).expect("serialize");
        assert!(
            json.get("tenant_id").is_none(),
            "tenant_id must be absent for several tenants"
        );
    }
}

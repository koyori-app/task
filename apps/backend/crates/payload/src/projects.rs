use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::projects;

#[derive(Debug, Clone, Serialize, ToSchema, serde::Deserialize)]
pub struct ProjectResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    #[schema(nullable)]
    pub icon_emoji: Option<String>,
    #[schema(nullable)]
    pub icon_url: Option<String>,
    pub key: String,
    pub is_personal: bool,
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub personal_owner_id: Option<Uuid>,
    /// テナントのメンバーを継承するか。真ならテナントのメンバーは全員メンバーの役で入れ、
    /// プロジェクトのメンバーに載せた人は載せた役で入る。偽なら載せた人だけが入れる。
    /// 個人プロジェクトでは使わない（本人だけが入れる）
    ///
    /// この欄を返さない旧いサーバーの応答も CLI が読めるよう、無ければ真（旧いサーバーの既定）とする。
    /// サーバーは常に返すので、OpenAPI の上では必須のまま残す
    #[serde(default = "inherits_tenant_members_by_default")]
    #[schema(required = true)]
    pub inherit_tenant_members: bool,
}

fn inherits_tenant_members_by_default() -> bool {
    true
}

impl From<projects::Model> for ProjectResponse {
    fn from(model: projects::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            description: model.description,
            tenant_id: model.tenant_id,
            icon_emoji: model.icon_emoji,
            icon_url: model.icon_url,
            key: model.key,
            is_personal: model.is_personal,
            personal_owner_id: model.personal_owner_id,
            inherit_tenant_members: model.inherit_tenant_members,
        }
    }
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    #[validate(length(min = 1))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[validate(length(max = 8))]
    pub icon_emoji: Option<String>,
    #[validate(url)]
    pub icon_url: Option<String>,
    pub key: Option<String>,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct UpdateProjectRequest {
    #[validate(length(min = 1))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[validate(length(max = 8))]
    pub icon_emoji: Option<String>,
    #[validate(url)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub clear_icon_emoji: bool,
    #[serde(default)]
    pub clear_icon_url: bool,
    /// テナントのメンバーを継承するか。変えられるのはプロジェクトのメンバーを変えられる人
    /// （テナントのオーナー、プロジェクトの Admin、プロジェクトに入れるテナントの Admin）だけ
    pub inherit_tenant_members: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_response_without_inheritance_field_reads_as_inheriting() {
        // inherit_tenant_members を返さない旧いサーバーの応答
        let project: ProjectResponse = serde_json::from_value(serde_json::json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "name": "App",
            "description": "",
            "tenant_id": "22222222-2222-4222-8222-222222222222",
            "icon_emoji": null,
            "icon_url": null,
            "key": "APP",
            "is_personal": false,
            "personal_owner_id": null,
        }))
        .expect("旧いサーバーの応答を読める");
        assert!(project.inherit_tenant_members);
    }
}

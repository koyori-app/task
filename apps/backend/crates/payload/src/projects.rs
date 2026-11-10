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
    pub inherit_tenant_members: bool,
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

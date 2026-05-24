use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

use crate::entities::scopes::ScopeList;

/// JSONB `allowed_project_ids` を `Vec<Uuid>` に復元する。
pub fn parse_allowed_project_ids(value: &serde_json::Value) -> Option<Vec<Uuid>> {
    serde_json::from_value(value.clone()).ok()
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema)]
#[sea_orm(table_name = "personal_tokens")]
#[schema(as=crate::entities::personal_tokens::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)] // auto_incrementを無効にする
    #[schema(value_type = String, format="uuid")]  // OpenAPIでUUIDとして扱うための属性
    pub id: Uuid,
    pub name: String,
    pub token_last_four: String,
    #[sea_orm(indexed)]
    #[schema(ignore)]
    #[serde(skip_serializing)]
    pub token_hash: String,
    #[schema(value_type = String, format="date-time", nullable)]
    pub expires_at: Option<DateTimeWithTimeZone>,
    #[schema(value_type = String, format="date-time", nullable)]
    pub last_used_at: Option<DateTimeWithTimeZone>,
    pub revoked: bool,
    #[schema(value_type = String, format="uuid")]
    pub user_id: Uuid,
    pub scopes: ScopeList,
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    /// JSONB として保存（`NULL` = テナント内全プロジェクト）。復元は `parse_allowed_project_ids`。
    #[sea_orm(nullable)]
    pub allowed_project_ids: Option<serde_json::Value>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(belongs_to = "super::users::Entity", from = "Column::UserId", to = "super::users::Column::Id", on_update = "NoAction", on_delete = "Cascade")]
    Users,
    #[sea_orm(
        belongs_to = "super::tenants::Entity",
        from = "Column::TenantId",
        to = "super::tenants::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Tenants,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl Related<super::tenants::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tenants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

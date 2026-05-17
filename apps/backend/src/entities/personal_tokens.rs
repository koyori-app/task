use crate::entities::scopes::ScopeList;
use sea_orm::entity::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, PartialEq, DeriveEntityModel, Eq, ToSchema)]
#[sea_orm(table_name = "personal_tokens")]
#[schema(as=crate::entities::personal_tokens::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)] // auto_incrementを無効にする
    #[schema(value_type = String, format="uuid")] // OpenAPIでUUIDとして扱うための属性
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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

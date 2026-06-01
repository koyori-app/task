use sea_orm::entity::prelude::*;
use serde::Serialize;
use utoipa::ToSchema; // Scalar/OpenAPI用

#[derive(Clone, Serialize, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema)]
#[sea_orm(table_name = "users")]
#[schema(as=crate::entities::users::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)] // auto_incrementを無効にする
    #[schema(value_type = String, format="uuid")] // OpenAPIでUUIDとして扱うための属性
    pub id: Uuid,
    #[schema(value_type = String, format="username")]
    pub username: String,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub bio: Option<String>,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub avatar_url: Option<String>,
    #[sea_orm(unique)]
    #[schema(value_type = String, format="email")]
    pub email: String,
    /// メールアドレスの確認が済んでいるかどうか
    pub email_verified: bool,
    #[schema(ignore)]
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_admin: bool,
    pub is_suspended: bool,
    #[sea_orm(nullable)]
    #[schema(value_type = String, format = "date-time", nullable)]
    pub sessions_revoked_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::tenants::Entity")]
    Tenants,
}

impl Related<super::tenants::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tenants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

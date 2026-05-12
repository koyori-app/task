use sea_orm::entity::prelude::*;
use utoipa::ToSchema; // Scalar/OpenAPI用

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)] // auto_incrementを無効にする
    #[schema(value_type = String, format="uuid")]  // OpenAPIでUUIDとして扱うための属性
    pub id: Uuid,
    #[schema(value_type = String, format="username")]
    pub username: String,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub bio: String,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub avatar_url: Option<String>,
    #[schema(value_type = String, format="email")]
    pub email: String,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub password_hash: Option<String>,
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

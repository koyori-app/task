use sea_orm::entity::prelude::*;
use utoipa::ToSchema; // Scalar/OpenAPI用

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema)]
#[sea_orm(table_name = "labels")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)] // auto_incrementを無効にする
    #[schema(value_type = String, format="uuid")]  // OpenAPIでUUIDとして扱うための属性
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub icon_url: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

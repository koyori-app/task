use sea_orm::entity::prelude::*;
use utoipa::ToSchema; // Scalar/OpenAPI用

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema)]
#[sea_orm(table_name = "tenants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)] // auto_incrementを無効にする
    #[schema(value_type = String, format="uuid")]  // OpenAPIでUUIDとして扱うための属性
    pub id: Uuid,
    pub display_id: String,
    pub name: String,
    pub description: String,
    pub icon_url: String,
    #[schema(value_type = String, format="uuid")]
    pub owner_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(belongs_to = "super::users::Entity", from = "Column::OwnerId", to = "super::users::Column::Id", on_update = "NoAction", on_delete = "Cascade")]
    Users,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

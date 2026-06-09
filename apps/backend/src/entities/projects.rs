use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "projects")]
#[schema(as=crate::entities::projects::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format="uuid")]
    pub id: Uuid,
    pub name: String,
    pub description: String,
    #[schema(value_type = String, format="uuid")]
    pub tenant_id: Uuid,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub icon_emoji: Option<String>,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub icon_url: Option<String>,
    pub key: String,
    #[serde(default)]
    pub is_personal: bool,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub personal_owner_id: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::tenants::Entity",
        from = "Column::TenantId",
        to = "super::tenants::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Tenants,
}

impl Related<super::tenants::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tenants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

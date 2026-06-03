use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "audit_logs")]
#[schema(as = crate::entities::audit_logs::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[sea_orm(nullable)]
    #[schema(nullable, value_type = Option<String>, format = "uuid")]
    pub actor_id: Option<Uuid>,
    pub actor_type: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    #[sea_orm(nullable)]
    #[schema(nullable, value_type = Option<String>, format = "uuid")]
    pub tenant_id: Option<Uuid>,
    #[sea_orm(nullable, column_type = "JsonBinary")]
    #[schema(nullable, value_type = Option<serde_json::Value>)]
    pub metadata: Option<Json>,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub ip_address: Option<String>,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub user_agent: Option<String>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ActorId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Users,
    #[sea_orm(
        belongs_to = "super::tenants::Entity",
        from = "Column::TenantId",
        to = "super::tenants::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
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

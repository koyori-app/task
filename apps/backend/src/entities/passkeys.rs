use sea_orm::entity::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, ToSchema)]
#[sea_orm(table_name = "passkeys")]
#[schema(as = crate::entities::passkeys::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    #[schema(ignore)]
    #[serde(skip_serializing)]
    pub credential_id: Vec<u8>,
    #[schema(ignore)]
    #[serde(skip_serializing)]
    pub public_key: Vec<u8>,
    #[schema(ignore)]
    #[serde(skip_serializing)]
    pub aaguid: Option<Vec<u8>>,
    pub sign_count: i64,
    pub name: String,
    #[schema(value_type = String, format = "date-time", nullable)]
    pub last_used_at: Option<DateTimeWithTimeZone>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeWithTimeZone,
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

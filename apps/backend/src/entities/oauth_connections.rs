use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "oauth_connections")]
#[schema(as = crate::entities::oauth_connections::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub provider: String,
    pub provider_user_id: String,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub provider_email: Option<String>,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub instance_url: Option<String>,
    #[sea_orm(nullable)]
    #[schema(ignore)]
    #[serde(skip_serializing)]
    pub access_token_enc: Option<String>,
    #[sea_orm(nullable)]
    #[schema(ignore)]
    #[serde(skip_serializing)]
    pub refresh_token_enc: Option<String>,
    #[sea_orm(nullable)]
    #[schema(value_type = String, format = "date-time", nullable)]
    pub token_expires_at: Option<DateTimeWithTimeZone>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeWithTimeZone,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: DateTimeWithTimeZone,
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

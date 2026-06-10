use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "time_logs")]
#[schema(as = crate::entities::time_logs::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub logged_minutes: i32,
    #[schema(value_type = String, format = "date")]
    pub logged_at: chrono::NaiveDate,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub note: Option<String>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::tasks::Entity",
        from = "Column::TaskId",
        to = "super::tasks::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Tasks,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::tasks::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tasks.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

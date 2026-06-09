use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "task_comments")]
#[schema(as = crate::entities::task_comments::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub body: String,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub parent_comment_id: Option<Uuid>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: DateTimeUtc,
    #[sea_orm(nullable)]
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub deleted_at: Option<DateTimeUtc>,
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
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentCommentId",
        to = "Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    ParentComment,
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

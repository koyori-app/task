use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "task_attachments")]
#[schema(as = crate::entities::task_attachments::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub drive_file_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub created_by: Uuid,
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
        belongs_to = "super::drive_files::Entity",
        from = "Column::DriveFileId",
        to = "super::drive_files::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    DriveFiles,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedBy",
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

impl Related<super::drive_files::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DriveFiles.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

//! Task attachments entity — schema-first generated output re-exported for stable module path.
pub use super::_generated::task_attachments::*;

use sea_orm::entity::prelude::*;

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

use sea_orm::entity::prelude::*;
use sea_orm::ActiveValue;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum StorageType {
    #[sea_orm(string_value = "s3")]
    S3,
    #[sea_orm(string_value = "local")]
    Local,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "drive_files")]
#[schema(as = crate::entities::drive_files::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    pub storage_type: StorageType,
    pub storage_key: String,
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    #[sea_orm(nullable)]
    #[schema(value_type = String, format = "uuid", nullable)]
    pub project_id: Option<Uuid>,
    #[schema(value_type = String, format = "uuid")]
    pub uploader_id: Uuid,
    #[sea_orm(nullable)]
    #[schema(value_type = String, format = "uuid", nullable)]
    pub folder_id: Option<Uuid>,
    #[schema(value_type = String, format = "date-time")]
    #[sea_orm(default_expr = "Expr::current_timestamp()")]
    pub created_at: DateTimeWithTimeZone,
}

/// CHECK: `project_id IS NULL OR folder_id IS NOT NULL`
pub fn validate_project_folder_constraint(
    project_id: Option<Uuid>,
    folder_id: Option<Uuid>,
) -> Result<(), DbErr> {
    if project_id.is_some() && folder_id.is_none() {
        return Err(DbErr::Custom(
            "drive_files: project_id requires folder_id (CHECK constraint)".into(),
        ));
    }
    Ok(())
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
    #[sea_orm(
        belongs_to = "super::projects::Entity",
        from = "Column::ProjectId",
        to = "super::projects::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Projects,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UploaderId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Users,
    #[sea_orm(
        belongs_to = "super::drive_folders::Entity",
        from = "Column::FolderId",
        to = "super::drive_folders::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    DriveFolders,
}

impl Related<super::tenants::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tenants.def()
    }
}

impl Related<super::projects::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Projects.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl Related<super::drive_folders::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DriveFolders.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, _insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let project_id = active_option_uuid(&self.project_id);
        let folder_id = active_option_uuid(&self.folder_id);
        if let (Some(project_id), Some(folder_id)) = (project_id, folder_id) {
            validate_project_folder_constraint(project_id, folder_id)?;
        }
        Ok(self)
    }
}

fn active_option_uuid(value: &ActiveValue<Option<Uuid>>) -> Option<Option<Uuid>> {
    match value {
        ActiveValue::Set(v) => Some(v.clone()),
        ActiveValue::Unchanged(v) => Some(v.clone()),
        ActiveValue::NotSet => None,
    }
}

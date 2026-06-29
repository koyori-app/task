//! Drive files entity — schema-first with hand-written DeriveActiveEnum and validation.
use sea_orm::ActiveValue;
use sea_orm::entity::prelude::*;
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

pub use super::_generated::drive_files::*;

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

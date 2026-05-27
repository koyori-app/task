use sea_orm::entity::prelude::*;
use sea_orm::ActiveValue;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum SharePermission {
    #[sea_orm(string_value = "viewer")]
    Viewer,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "drive_folder_shares")]
#[schema(as = crate::entities::drive_folder_shares::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub folder_id: Uuid,
    #[sea_orm(nullable)]
    #[schema(value_type = String, format = "uuid", nullable)]
    pub shared_with_user_id: Option<Uuid>,
    #[sea_orm(nullable, unique)]
    #[schema(nullable)]
    pub share_token: Option<String>,
    pub permission: SharePermission,
    #[schema(value_type = String, format = "uuid")]
    pub created_by: Uuid,
    #[sea_orm(nullable)]
    #[schema(value_type = String, format = "date-time", nullable)]
    pub expires_at: Option<DateTimeWithTimeZone>,
    #[schema(value_type = String, format = "date-time")]
    #[sea_orm(default_expr = "Expr::current_timestamp()")]
    pub created_at: DateTimeWithTimeZone,
}

/// CHECK: `(shared_with_user_id IS NOT NULL) XOR (share_token IS NOT NULL)`
pub fn validate_share_target_xor(
    shared_with_user_id: Option<Uuid>,
    share_token: Option<&str>,
) -> Result<(), DbErr> {
    let has_user = shared_with_user_id.is_some();
    let has_token = share_token.is_some();
    if has_user == has_token {
        return Err(DbErr::Custom(
            "drive_folder_shares: exactly one of shared_with_user_id or share_token must be set (CHECK constraint)".into(),
        ));
    }
    Ok(())
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::drive_folders::Entity",
        from = "Column::FolderId",
        to = "super::drive_folders::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    DriveFolders,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::SharedWithUserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    SharedWithUser,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedBy",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    CreatedByUser,
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
        let shared_with_user_id = active_option_uuid(&self.shared_with_user_id);
        let share_token = active_option_string(&self.share_token);
        if let (Some(shared_with_user_id), Some(share_token)) =
            (shared_with_user_id, share_token)
        {
            let token_ref = share_token.as_deref();
            validate_share_target_xor(shared_with_user_id, token_ref)?;
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

fn active_option_string(value: &ActiveValue<Option<String>>) -> Option<Option<String>> {
    match value {
        ActiveValue::Set(v) => Some(v.clone()),
        ActiveValue::Unchanged(v) => Some(v.clone()),
        ActiveValue::NotSet => None,
    }
}

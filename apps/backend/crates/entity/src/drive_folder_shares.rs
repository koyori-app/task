//! Drive folder shares entity — schema-first with hand-written DeriveActiveEnum and validation.
use sea_orm::ActiveValue;
use sea_orm::entity::prelude::*;
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

pub use super::_generated::drive_folder_shares::*;

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

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, _insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let shared_with_user_id = active_option_uuid(&self.shared_with_user_id);
        let share_token = active_option_string(&self.share_token);
        if let (Some(shared_with_user_id), Some(share_token)) = (shared_with_user_id, share_token) {
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

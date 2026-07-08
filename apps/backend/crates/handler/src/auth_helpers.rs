use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, prelude::Uuid};

use crate::AppState;
use crate::error::AppError;
use entity::project_members;

// 実装は service 側に一本化（レビュー指摘: 同一実装の重複解消）。
pub use service::drive::is_tenant_owner;

pub async fn require_member_or_owner(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_owner(&state.db, tenant_id, user_id).await? {
        return Ok(());
    }
    let is_member = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?
        .is_some();
    if is_member {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

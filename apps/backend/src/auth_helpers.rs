use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, prelude::Uuid};

use crate::entities::{project_members, tenants};
use crate::error::AppError;
use crate::AppState;

pub async fn is_tenant_owner(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(tenant.owner_id == user_id)
}

pub async fn require_tenant_owner(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_owner(state, tenant_id, user_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn require_member_or_owner(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_owner(state, tenant_id, user_id).await? {
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

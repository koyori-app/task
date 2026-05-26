use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter,
};
use sea_orm::prelude::Uuid;
use serde::Deserialize;
use validator::Validate;

use crate::entities::{project_members, projects, scopes::Scope, tenants, users};
use crate::entities::project_members::ProjectRole;
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::AppState;

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct AddMemberRequest {
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub role: ProjectRole,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateMemberRequest {
    pub role: ProjectRole,
}

async fn get_project_in_tenant(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
) -> Result<projects::Model, AppError> {
    projects::Entity::find_by_id(project_id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

async fn is_tenant_owner(
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

pub(crate) async fn require_project_admin(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let _ = get_project_in_tenant(state, tenant_id, project_id).await?;
    if is_tenant_owner(state, tenant_id, user_id).await? {
        return Ok(());
    }
    let member = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?;
    match member {
        Some(m) if m.role == ProjectRole::Admin => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

async fn find_member(
    state: &AppState,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<project_members::Model, AppError> {
    project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

async fn count_admins(state: &AppState, project_id: Uuid) -> Result<u64, AppError> {
    use sea_orm::PaginatorTrait;
    Ok(project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::Role.eq(ProjectRole::Admin))
        .count(&state.db)
        .await?)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    summary = "プロジェクトメンバー一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "メンバー一覧", body = [project_members::Model]),
        CrudErrors,
    )
)]
pub async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<project_members::Model>>, AppError> {
    auth.require_scope(Scope::ReadProject)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_project_admin(&state, tenant_id, project_id, auth.user_id).await?;
    let members = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .all(&state.db)
        .await?;
    Ok(Json(members))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    summary = "プロジェクトメンバーを追加",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = AddMemberRequest,
    responses(
        (status = 201, description = "追加されたメンバー", body = project_members::Model),
        (status = 409, description = "既にメンバーとして登録済み", body = ServerError),
        CrudErrors,
    )
)]
pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<AddMemberRequest>>,
) -> Result<(StatusCode, Json<project_members::Model>), AppError> {
    auth.require_scope(Scope::WriteProject)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_project_admin(&state, tenant_id, project_id, auth.user_id).await?;

    users::Entity::find_by_id(payload.user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let existing = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::UserId.eq(payload.user_id))
        .one(&state.db)
        .await?;
    if existing.is_some() {
        return Err(AppError::Conflict);
    }

    let member = project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(payload.user_id),
        role: Set(payload.role),
    };
    let model = member.insert(&state.db).await?;
    Ok((StatusCode::CREATED, Json(model)))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{user_id}",
    summary = "プロジェクトメンバーの権限を変更",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    request_body = UpdateMemberRequest,
    responses(
        (status = 200, description = "更新後のメンバー", body = project_members::Model),
        (status = 409, description = "最後のAdminは降格できません", body = ServerError),
        CrudErrors,
    )
)]
pub async fn update_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, member_user_id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateMemberRequest>>,
) -> Result<Json<project_members::Model>, AppError> {
    auth.require_scope(Scope::WriteProject)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_project_admin(&state, tenant_id, project_id, auth.user_id).await?;
    let current = find_member(&state, project_id, member_user_id).await?;
    if current.role == ProjectRole::Admin
        && payload.role != ProjectRole::Admin
        && count_admins(&state, project_id).await? <= 1
    {
        return Err(AppError::Conflict);
    }
    let mut active: project_members::ActiveModel = current.into();
    active.role = Set(payload.role);
    let updated = active.update(&state.db).await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{user_id}",
    summary = "プロジェクトメンバーを削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        (status = 409, description = "最後のAdminは削除できません", body = ServerError),
        CrudErrors,
    )
)]
pub async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, member_user_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(Scope::WriteProject)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_project_admin(&state, tenant_id, project_id, auth.user_id).await?;
    let member = find_member(&state, project_id, member_user_id).await?;
    if member.role == ProjectRole::Admin && count_admins(&state, project_id).await? <= 1 {
        return Err(AppError::Conflict);
    }
    project_members::Entity::delete_by_id(member.id)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

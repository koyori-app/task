use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, prelude::Uuid};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    entities::{projects, tasks, tenants},
    error::AppError,
    extractors::AdminUser,
    handlers::admin_audit::record_audit,
    openapi::CrudErrors,
};

#[derive(Serialize, ToSchema)]
pub struct AdminTenantListResponse {
    pub tenants: Vec<tenants::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminProjectListResponse {
    pub projects: Vec<projects::Model>,
}

#[derive(Serialize, ToSchema)]
pub struct AdminTaskListResponse {
    pub tasks: Vec<tasks::Model>,
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Admin Tenants",
    summary = "全テナント一覧（管理者）",
    responses(
        (status = 200, description = "テナント一覧", body = AdminTenantListResponse),
        CrudErrors,
    )
)]
pub async fn list_tenants(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<AdminTenantListResponse>, AppError> {
    let tenants = tenants::Entity::find()
        .order_by_asc(tenants::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(AdminTenantListResponse { tenants }))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Admin Tenants",
    summary = "テナント詳細（管理者）",
    params(("id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "テナント", body = tenants::Model),
        CrudErrors,
    )
)]
pub async fn get_tenant(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<tenants::Model>, AppError> {
    let tenant = tenants::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(tenant))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Admin Tenants",
    summary = "テナント強制削除（管理者）",
    params(("id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_tenant(
    State(state): State<AppState>,
    admin: AdminUser,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let tenant = tenants::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    tenants::Entity::delete_by_id(id).exec(&state.db).await?;

    record_audit(
        &state.db,
        admin.user_id,
        "tenant.delete",
        "tenant",
        &id.to_string(),
        Some(id),
        Some(serde_json::json!({ "tenant_name": tenant.name })),
        &headers,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{tenant_id}/projects",
    tag = "Admin Tenants",
    summary = "テナント配下プロジェクト一覧（管理者・読取専用）",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "プロジェクト一覧", body = AdminProjectListResponse),
        CrudErrors,
    )
)]
pub async fn list_tenant_projects(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<AdminProjectListResponse>, AppError> {
    if tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }
    let projects = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .order_by_asc(projects::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(AdminProjectListResponse { projects }))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{tenant_id}/projects/{project_id}/tasks",
    tag = "Admin Tenants",
    summary = "プロジェクト配下タスク一覧（管理者・読取専用）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "タスク一覧", body = AdminTaskListResponse),
        CrudErrors,
    )
)]
pub async fn list_tenant_project_tasks(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<AdminTaskListResponse>, AppError> {
    let project_exists = projects::Entity::find_by_id(project_id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .is_some();
    if !project_exists {
        return Err(AppError::NotFound);
    }
    let tasks = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .order_by_desc(tasks::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(AdminTaskListResponse { tasks }))
}

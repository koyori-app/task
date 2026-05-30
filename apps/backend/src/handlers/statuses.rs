use axum::{Json, extract::{Path, State}, http::StatusCode};
use axum_valid::Valid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, TransactionTrait, prelude::Uuid,
};
use sea_orm::sea_query::Expr;
use serde::Deserialize;
use std::collections::HashSet;
use utoipa::ToSchema;
use validator::Validate;

use crate::auth_helpers::require_member_or_owner;
use crate::entities::{project_statuses, tasks};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::AppState;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateStatusRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(min = 7, max = 7))]
    pub color: String,
    pub position: i16,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_done_state: bool,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateStatusRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    #[validate(length(min = 7, max = 7))]
    pub color: Option<String>,
    pub position: Option<i16>,
    pub is_default: Option<bool>,
    pub is_done_state: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct ReorderRequest {
    pub ids: Vec<Uuid>,
}

#[derive(Deserialize, ToSchema)]
pub struct DeleteStatusQuery {
    pub migrate_to_status_id: Option<Uuid>,
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Statuses",
    summary = "カスタムステータス一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "ステータス一覧（position 順）", body = [project_statuses::Model]),
        CrudErrors,
    )
)]
pub async fn list_statuses(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<project_statuses::Model>>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let statuses = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .order_by_asc(project_statuses::Column::Position)
        .all(&state.db)
        .await?;
    Ok(Json(statuses))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Statuses",
    summary = "カスタムステータス作成",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = CreateStatusRequest,
    responses(
        (status = 201, description = "作成されたステータス", body = project_statuses::Model),
        CrudErrors,
    )
)]
pub async fn create_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateStatusRequest>>,
) -> Result<(StatusCode, Json<project_statuses::Model>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let status = project_statuses::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set(payload.name),
        color: Set(payload.color),
        position: Set(payload.position),
        is_default: Set(payload.is_default),
        is_done_state: Set(payload.is_done_state),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(status)))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Statuses",
    summary = "カスタムステータス更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "ステータスID"),
    ),
    request_body = UpdateStatusRequest,
    responses(
        (status = 200, description = "更新後のステータス", body = project_statuses::Model),
        CrudErrors,
    )
)]
pub async fn update_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateStatusRequest>>,
) -> Result<Json<project_statuses::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let status = project_statuses::Entity::find_by_id(id)
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut active: project_statuses::ActiveModel = status.into();
    if payload.is_default == Some(true) {
        project_statuses::Entity::update_many()
            .col_expr(project_statuses::Column::IsDefault, Expr::value(false))
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .filter(project_statuses::Column::Id.ne(id))
            .exec(&state.db)
            .await?;
    }
    if let Some(v) = payload.name { active.name = Set(v); }
    if let Some(v) = payload.color { active.color = Set(v); }
    if let Some(v) = payload.position { active.position = Set(v); }
    if let Some(v) = payload.is_default { active.is_default = Set(v); }
    if let Some(v) = payload.is_done_state { active.is_done_state = Set(v); }
    Ok(Json(active.update(&state.db).await?))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/reorder",
    tag = "Statuses",
    summary = "ステータス並び順一括更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = ReorderRequest,
    responses(
        (status = 200, description = "並び替え後のステータス一覧", body = [project_statuses::Model]),
        CrudErrors,
    )
)]
pub async fn reorder_statuses(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<ReorderRequest>,
) -> Result<Json<Vec<project_statuses::Model>>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let existing = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .all(&state.db)
        .await?;
    if payload.ids.len() != existing.len() {
        return Err(AppError::BadRequest);
    }
    let existing_ids: HashSet<Uuid> = existing.iter().map(|s| s.id).collect();
    if payload.ids.len() != payload.ids.iter().collect::<HashSet<_>>().len()
        || payload.ids.iter().any(|id| !existing_ids.contains(id))
    {
        return Err(AppError::BadRequest);
    }

    let txn = state.db.begin().await?;
    for (pos, sid) in payload.ids.iter().enumerate() {
        let status = project_statuses::Entity::find_by_id(*sid)
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .one(&txn)
            .await?
            .ok_or(AppError::NotFound)?;
        let mut active: project_statuses::ActiveModel = status.into();
        active.position = Set(pos as i16);
        active.update(&txn).await?;
    }
    txn.commit().await?;

    let updated = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .order_by_asc(project_statuses::Column::Position)
        .all(&state.db)
        .await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Statuses",
    summary = "カスタムステータス削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "ステータスID"),
        ("migrate_to_status_id" = Option<Uuid>, Query, description = "移行先ステータスID（タスクが存在する場合は必須）"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    axum::extract::Query(q): axum::extract::Query<DeleteStatusQuery>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    project_statuses::Entity::find_by_id(id)
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let task_count = tasks::Entity::find()
        .filter(tasks::Column::StatusId.eq(id))
        .filter(tasks::Column::DeletedAt.is_null())
        .count(&state.db)
        .await?;

    if task_count > 0 {
        let migrate_to = q.migrate_to_status_id
            .ok_or(AppError::BadRequest)?;
        if migrate_to == id {
            return Err(AppError::BadRequest);
        }
        // Verify target status belongs to same project
        project_statuses::Entity::find_by_id(migrate_to)
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;

        let txn = state.db.begin().await?;
        let affected = tasks::Entity::find()
            .filter(tasks::Column::StatusId.eq(id))
            .filter(tasks::Column::DeletedAt.is_null())
            .all(&txn)
            .await?;
        for t in affected {
            let mut active: tasks::ActiveModel = t.into();
            active.status_id = Set(migrate_to);
            active.update(&txn).await?;
        }
        project_statuses::Entity::delete_by_id(id).exec(&txn).await?;
        txn.commit().await?;
    } else {
        project_statuses::Entity::delete_by_id(id).exec(&state.db).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

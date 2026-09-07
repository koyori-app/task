use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::sea_query::{CaseStatement, Expr, Func, LockType};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait, prelude::Uuid,
};
use std::collections::HashSet;

use crate::AppState;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use entity::{project_statuses, projects, tasks};
use payload::statuses::*;
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
        (status = 200, description = "ステータス一覧（position 順）", body = [ProjectStatusResponse]),
        CrudErrors,
    )
)]
pub async fn list_statuses(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<ProjectStatusResponse>>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let statuses = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .order_by_asc(project_statuses::Column::Position)
        .all(&state.db)
        .await?;
    Ok(Json(statuses.into_iter().map(Into::into).collect()))
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
        (status = 201, description = "作成されたステータス", body = ProjectStatusResponse),
        CrudErrors,
    )
)]
pub async fn create_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateStatusRequest>>,
) -> Result<(StatusCode, Json<ProjectStatusResponse>), AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    // 完了ステータスは複数持てる。既定の完了（「完了にする」操作の移動先）だけが
    // プロジェクト内で 1 つに限られ、完了ステータスにしか付けられない。
    if payload.is_default_done && !payload.is_done_state {
        return Err(AppError::BadRequest);
    }
    let txn = state.db.begin().await?;
    // A status row cannot be the mutex here: concurrent creates can insert rows
    // outside the other transaction's locked snapshot. Lock the stable project
    // row, using the same lock order as update_status, before reading statuses.
    projects::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    let statuses = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .all(&txn)
        .await?;
    if payload.is_default {
        project_statuses::Entity::update_many()
            .col_expr(project_statuses::Column::IsDefault, Expr::value(false))
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .exec(&txn)
            .await?;
    }
    // 完了ステータスが 1 つも無いプロジェクトへ足す 1 つ目は既定の完了にする。
    let is_first_done_state =
        payload.is_done_state && !statuses.iter().any(|status| status.is_done_state);
    let is_default_done = payload.is_default_done || is_first_done_state;
    if is_default_done {
        project_statuses::Entity::update_many()
            .col_expr(project_statuses::Column::IsDefaultDone, Expr::value(false))
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .exec(&txn)
            .await?;
    }
    let status = project_statuses::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set(payload.name),
        color: Set(payload.color),
        position: Set(payload.position),
        is_default: Set(payload.is_default),
        is_done_state: Set(payload.is_done_state),
        is_default_done: Set(is_default_done),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok((StatusCode::CREATED, Json(status.into())))
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
        (status = 200, description = "更新後のステータス", body = ProjectStatusResponse),
        CrudErrors,
    )
)]
pub async fn update_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateStatusRequest>>,
) -> Result<Json<ProjectStatusResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let txn = state.db.begin().await?;
    // Serialize status flag changes for the project. In particular, two concurrent requests
    // must not both observe themselves as the next Done state.
    projects::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    let statuses = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .all(&txn)
        .await?;
    let status = statuses
        .iter()
        .find(|status| status.id == id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    let old_is_done_state = status.is_done_state;
    if payload.is_default == Some(false) && status.is_default {
        return Err(AppError::BadRequest);
    }
    if payload.is_default_done == Some(false) && status.is_default_done {
        return Err(AppError::BadRequest);
    }
    // 完了ステータスは複数持てるが、0 個にはできない（完了の行き先が無くなる）。
    let demoting_done = payload.is_done_state == Some(false) && old_is_done_state;
    if demoting_done
        && !statuses
            .iter()
            .any(|status| status.is_done_state && status.id != id)
    {
        return Err(AppError::BadRequest);
    }
    // 既定の完了は完了ステータスにしか付けられない。
    if payload.is_default_done == Some(true) && !payload.is_done_state.unwrap_or(old_is_done_state)
    {
        return Err(AppError::BadRequest);
    }
    let mut active: project_statuses::ActiveModel = status.into();
    if payload.is_default == Some(true) {
        project_statuses::Entity::update_many()
            .col_expr(project_statuses::Column::IsDefault, Expr::value(false))
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .filter(project_statuses::Column::Id.ne(id))
            .exec(&txn)
            .await?;
    }
    if payload.is_default_done == Some(true) {
        project_statuses::Entity::update_many()
            .col_expr(project_statuses::Column::IsDefaultDone, Expr::value(false))
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .filter(project_statuses::Column::Id.ne(id))
            .exec(&txn)
            .await?;
    }
    if demoting_done {
        // 完了でなくなったので、このステータスに居るタスクの完了時刻を消す。
        tasks::Entity::update_many()
            .col_expr(
                tasks::Column::CompletedAt,
                Expr::value(Option::<chrono::DateTime<chrono::Utc>>::None),
            )
            .filter(tasks::Column::StatusId.eq(id))
            .filter(tasks::Column::DeletedAt.is_null())
            .exec(&txn)
            .await?;
    }
    if payload.is_done_state == Some(true) && !old_is_done_state {
        tasks::Entity::update_many()
            .col_expr(
                tasks::Column::CompletedAt,
                Expr::expr(Func::coalesce([
                    Expr::col(tasks::Column::CompletedAt),
                    Expr::current_timestamp(),
                ])),
            )
            .filter(tasks::Column::StatusId.eq(id))
            .filter(tasks::Column::DeletedAt.is_null())
            .exec(&txn)
            .await?;
    }
    if let Some(v) = payload.name {
        active.name = Set(v);
    }
    if let Some(v) = payload.color {
        active.color = Set(v);
    }
    if let Some(v) = payload.position {
        active.position = Set(v);
    }
    if let Some(v) = payload.is_default {
        active.is_default = Set(v);
    }
    if let Some(v) = payload.is_done_state {
        active.is_done_state = Set(v);
    }
    if let Some(v) = payload.is_default_done {
        active.is_default_done = Set(v);
    }
    if demoting_done {
        active.is_default_done = Set(false);
    }
    let updated = active.update(&txn).await?;

    txn.commit().await?;
    Ok(Json(updated.into()))
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
        (status = 200, description = "並び替え後のステータス一覧", body = [ProjectStatusResponse]),
        CrudErrors,
    )
)]
pub async fn reorder_statuses(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<ReorderRequest>,
) -> Result<Json<Vec<ProjectStatusResponse>>, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let txn = state.db.begin().await?;
    // Same lock order as create_status / update_status: take the stable project
    // row first, then the whole status set, so concurrent status writes for this
    // project serialize instead of racing on an unlocked read.
    projects::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    let existing = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .all(&txn)
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

    // Reassign every position in a single UPDATE via a CASE expression instead of
    // the previous per-id find + update loop (N+1). The payload is a bijection
    // onto the locked status set, and (project_id, position) has no unique
    // constraint, so reassigning the whole set at once cannot transiently collide.
    // The final `finally` keeps any unmatched row's current position, guarding the
    // NOT NULL column even though the bijection means every row is matched.
    let mut position_case = CaseStatement::new();
    for (pos, sid) in payload.ids.iter().enumerate() {
        position_case = position_case.case(
            project_statuses::Column::Id.eq(*sid),
            Expr::value(pos as i16),
        );
    }
    project_statuses::Entity::update_many()
        .col_expr(
            project_statuses::Column::Position,
            position_case
                .finally(Expr::col(project_statuses::Column::Position))
                .into(),
        )
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .exec(&txn)
        .await?;

    let updated = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .order_by_asc(project_statuses::Column::Position)
        .all(&txn)
        .await?;
    txn.commit().await?;
    Ok(Json(updated.into_iter().map(Into::into).collect()))
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
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let txn = state.db.begin().await?;
    let statuses = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .all(&txn)
        .await?;
    let status = statuses
        .iter()
        .find(|status| status.id == id)
        .cloned()
        .ok_or(AppError::NotFound)?;

    if status.is_default {
        return Err(AppError::BadRequest);
    }
    if status.is_done_state
        && statuses
            .iter()
            .filter(|status| status.is_done_state)
            .count()
            == 1
    {
        return Err(AppError::BadRequest);
    }

    let task_count = tasks::Entity::find()
        .filter(tasks::Column::StatusId.eq(id))
        .filter(tasks::Column::DeletedAt.is_null())
        .count(&txn)
        .await?;

    if task_count > 0 {
        let migrate_to = q.migrate_to_status_id.ok_or(AppError::BadRequest)?;
        if migrate_to == id {
            return Err(AppError::BadRequest);
        }
        // The locked snapshot also verifies that the target belongs to this project.
        let target_status = statuses
            .iter()
            .find(|status| status.id == migrate_to)
            .ok_or(AppError::NotFound)?;

        let mut update =
            tasks::Entity::update_many().col_expr(tasks::Column::StatusId, Expr::value(migrate_to));
        update = if target_status.is_done_state {
            update.col_expr(
                tasks::Column::CompletedAt,
                Expr::expr(Func::coalesce([
                    Expr::col(tasks::Column::CompletedAt),
                    Expr::current_timestamp(),
                ])),
            )
        } else {
            update.col_expr(
                tasks::Column::CompletedAt,
                Expr::value(Option::<chrono::DateTime<chrono::Utc>>::None),
            )
        };
        update
            .filter(tasks::Column::StatusId.eq(id))
            .filter(tasks::Column::DeletedAt.is_null())
            .exec(&txn)
            .await?;
        project_statuses::Entity::delete_by_id(id)
            .exec(&txn)
            .await?;
    } else {
        project_statuses::Entity::delete_by_id(id)
            .exec(&txn)
            .await?;
    }
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

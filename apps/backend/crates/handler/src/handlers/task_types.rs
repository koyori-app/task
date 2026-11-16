use crate::AppState;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use entity::project_task_types;
use payload::task_types::*;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait, prelude::Uuid, sea_query::LockType,
};
use service::ordering;
use service::task_types::find_in_project;

async fn list_ordered<C: sea_orm::ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<Vec<project_task_types::Model>, AppError> {
    Ok(project_task_types::Entity::find()
        .filter(project_task_types::Column::ProjectId.eq(project_id))
        .order_by_asc(project_task_types::Column::Position)
        .order_by_asc(project_task_types::Column::Id)
        .all(db)
        .await?)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Task Types",
    summary = "プロジェクトのタスク種別一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "並び順（position の昇順）のタスク種別一覧", body = [TaskTypeResponse]),
        CrudErrors,
    )
)]
pub async fn list_task_types(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<TaskTypeResponse>>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let list = list_ordered(&state.db, project_id).await?;
    Ok(Json(list.into_iter().map(Into::into).collect()))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Task Types",
    summary = "タスク種別作成",
    description = "並び順は末尾に付く。同じプロジェクトに同名の種別があると 409",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = CreateTaskTypeRequest,
    responses(
        (status = 201, description = "作成されたタスク種別", body = TaskTypeResponse),
        CrudErrors,
    )
)]
pub async fn create_task_type(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateTaskTypeRequest>>,
) -> Result<(StatusCode, Json<TaskTypeResponse>), AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    // 並べ替えと同じくプロジェクト行をロックしてから最大値を読み、同時の作成が同じ position を取らないようにする
    let txn = state.db.begin().await?;
    ordering::lock_project(&txn, project_id).await?;
    let max = project_task_types::Entity::find()
        .filter(project_task_types::Column::ProjectId.eq(project_id))
        .order_by_desc(project_task_types::Column::Position)
        .one(&txn)
        .await?
        .map(|t| t.position);
    let created = project_task_types::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set(payload.name),
        color: Set(payload.color),
        position: Set(ordering::next_unique_position(max)?),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Task Types",
    summary = "タスク種別更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "タスク種別ID"),
    ),
    request_body = UpdateTaskTypeRequest,
    responses(
        (status = 200, description = "更新後のタスク種別", body = TaskTypeResponse),
        CrudErrors,
    )
)]
pub async fn update_task_type(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateTaskTypeRequest>>,
) -> Result<Json<TaskTypeResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    // 他の書き込みと同じくプロジェクト行を先に取る（service::ordering のロック順）
    let txn = state.db.begin().await?;
    ordering::lock_project(&txn, project_id).await?;
    let task_type = find_in_project(&txn, project_id, id).await?;
    let mut active: project_task_types::ActiveModel = task_type.into();
    if let Some(v) = payload.name {
        active.name = Set(v);
    }
    if let Some(v) = payload.color {
        active.color = Set(v);
    }
    let updated = active.update(&txn).await?;
    txn.commit().await?;
    Ok(Json(updated.into()))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/reorder",
    tag = "Task Types",
    summary = "タスク種別の並び順一括更新",
    description = "プロジェクトの全種別の ID を、並べたい順に過不足なく渡す。足りない・余る・重複する・別プロジェクトの ID が混ざると 400",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = ReorderTaskTypesRequest,
    responses(
        (status = 200, description = "並び替え後のタスク種別一覧", body = [TaskTypeResponse]),
        CrudErrors,
    )
)]
pub async fn reorder_task_types(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<ReorderTaskTypesRequest>,
) -> Result<Json<Vec<TaskTypeResponse>>, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let txn = state.db.begin().await?;
    // ステータスの並べ替えと同じロック順（プロジェクト行 → 種別全体）で、同時の書き込みを直列にする
    ordering::lock_project(&txn, project_id).await?;
    let existing = project_task_types::Entity::find()
        .filter(project_task_types::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .all(&txn)
        .await?;
    ordering::validate_reorder(&payload.ids, existing.iter().map(|t| t.id))?;
    ordering::reassign_positions::<project_task_types::Entity, _>(
        &txn,
        project_task_types::Column::Id,
        project_task_types::Column::Position,
        project_task_types::Column::ProjectId,
        project_id,
        &payload.ids,
    )
    .await?;
    let list = list_ordered(&txn, project_id).await?;
    txn.commit().await?;
    Ok(Json(list.into_iter().map(Into::into).collect()))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Task Types",
    summary = "タスク種別削除",
    description = "付いていたタスクは残り、種別が未設定に戻る",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "タスク種別ID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_task_type(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    // プロジェクト行 → 種別の行 → タスク行（FK の ON DELETE SET NULL）の順にロックする（service::ordering）。
    // tasks.task_type_id は SET NULL で未設定に戻る
    let txn = state.db.begin().await?;
    ordering::lock_project(&txn, project_id).await?;
    let result = project_task_types::Entity::delete_many()
        .filter(project_task_types::Column::Id.eq(id))
        .filter(project_task_types::Column::ProjectId.eq(project_id))
        .exec(&txn)
        .await?;
    if result.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

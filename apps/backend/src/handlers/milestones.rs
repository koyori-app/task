use std::collections::HashSet;

use axum::{Json, extract::{Path, State}, http::StatusCode};
use axum_valid::Valid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, prelude::Uuid,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth_helpers::require_member_or_owner;
use crate::entities::{milestones, tasks};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::AppState;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateMilestoneRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub description: Option<String>,
    pub due_date: time::Date,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateMilestoneRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub clear_description: bool,
    pub due_date: Option<time::Date>,
}

#[derive(Serialize, ToSchema)]
pub struct MilestoneDetail {
    #[serde(flatten)]
    pub milestone: milestones::Model,
    pub progress_pct: u32,
    pub task_counts: TaskCounts,
}

#[derive(Serialize, ToSchema)]
pub struct TaskCounts {
    pub total: usize,
    pub done: usize,
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Milestones",
    summary = "マイルストーン一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "マイルストーン一覧", body = [milestones::Model]),
        CrudErrors,
    )
)]
pub async fn list_milestones(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<milestones::Model>>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadMilestone)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let list = milestones::Entity::find()
        .filter(milestones::Column::ProjectId.eq(project_id))
        .all(&state.db)
        .await?;
    Ok(Json(list))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Milestones",
    summary = "マイルストーン作成",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = CreateMilestoneRequest,
    responses(
        (status = 201, description = "作成されたマイルストーン", body = milestones::Model),
        CrudErrors,
    )
)]
pub async fn create_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateMilestoneRequest>>,
) -> Result<(StatusCode, Json<milestones::Model>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteMilestone)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let model = milestones::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set(payload.name),
        description: Set(payload.description),
        due_date: Set(payload.due_date),
        created_by: Set(auth.user_id),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
    }
    .insert(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(model)))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Milestones",
    summary = "マイルストーン取得（完了率含む）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "マイルストーンID"),
    ),
    responses(
        (status = 200, description = "マイルストーン詳細", body = MilestoneDetail),
        CrudErrors,
    )
)]
pub async fn get_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<MilestoneDetail>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadMilestone)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let milestone = milestones::Entity::find_by_id(id)
        .filter(milestones::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let all_tasks = tasks::Entity::find()
        .filter(tasks::Column::MilestoneId.eq(id))
        .filter(tasks::Column::DeletedAt.is_null())
        .all(&state.db)
        .await?;

    let total = all_tasks.len();
    let done = if total > 0 {
        // Fetch done statuses for this project
        let done_statuses: HashSet<Uuid> = crate::entities::project_statuses::Entity::find()
            .filter(crate::entities::project_statuses::Column::ProjectId.eq(project_id))
            .filter(crate::entities::project_statuses::Column::IsDoneState.eq(true))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect();
        all_tasks
            .iter()
            .filter(|t| done_statuses.contains(&t.status_id))
            .count()
    } else {
        0
    };

    let progress_pct = if total > 0 { (done * 100 / total) as u32 } else { 0 };

    Ok(Json(MilestoneDetail {
        milestone,
        progress_pct,
        task_counts: TaskCounts { total, done },
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Milestones",
    summary = "マイルストーン更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "マイルストーンID"),
    ),
    request_body = UpdateMilestoneRequest,
    responses(
        (status = 200, description = "更新後のマイルストーン", body = milestones::Model),
        CrudErrors,
    )
)]
pub async fn update_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateMilestoneRequest>>,
) -> Result<Json<milestones::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteMilestone)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let milestone = milestones::Entity::find_by_id(id)
        .filter(milestones::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut active: milestones::ActiveModel = milestone.into();
    if let Some(v) = payload.name { active.name = Set(v); }
    if payload.clear_description { active.description = Set(None); }
    else if let Some(v) = payload.description { active.description = Set(Some(v)); }
    if let Some(v) = payload.due_date { active.due_date = Set(v); }
    active.updated_at = Set(chrono::Utc::now());
    Ok(Json(active.update(&state.db).await?))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Milestones",
    summary = "マイルストーン削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "マイルストーンID"),
    ),
    responses(
        (status = 204, description = "削除しました（タスクの milestone_id は NULL リセット）"),
        CrudErrors,
    )
)]
pub async fn delete_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteMilestone)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    milestones::Entity::find_by_id(id)
        .filter(milestones::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    // tasks.milestone_id cascades to NULL via FK ON DELETE SET NULL
    milestones::Entity::delete_by_id(id).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

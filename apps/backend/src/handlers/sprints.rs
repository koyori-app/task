use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_valid::Valid;
use chrono::{Datelike, NaiveDate, NaiveTime};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait, prelude::Uuid,
};
use sea_orm::sea_query::LockType;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth_helpers::require_member_or_owner;
use crate::utils::db::is_postgres_unique_violation;
use crate::entities::{project_statuses, sprints, tasks};
use crate::entities::sprints::SprintStatus;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::AppState;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateSprintRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub goal: Option<String>,
    #[schema(value_type = String, example = "2026-06-01")]
    pub start_date: NaiveDate,
    #[schema(value_type = String, example = "2026-06-14")]
    pub end_date: NaiveDate,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateSprintRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub goal: Option<String>,
    #[serde(default)]
    pub clear_goal: bool,
    #[schema(value_type = Option<String>, example = "2026-06-01")]
    pub start_date: Option<NaiveDate>,
    #[schema(value_type = Option<String>, example = "2026-06-14")]
    pub end_date: Option<NaiveDate>,
}

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct ListSprintsQuery {
    pub status: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CompleteSprintRequest {
    #[schema(value_type = Option<String>, format = "uuid")]
    pub move_incomplete_to_sprint_id: Option<Uuid>,
    #[serde(default)]
    pub move_incomplete_to_backlog: bool,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct AssignTasksRequest {
    #[validate(length(min = 1))]
    pub task_ids: Vec<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub struct SprintTaskCounts {
    pub total: usize,
    pub done: usize,
    pub in_progress: usize,
}

#[derive(Serialize, ToSchema)]
pub struct BurndownPoint {
    #[schema(value_type = String, example = "2026-06-01")]
    pub date: time::Date,
    pub ideal_remaining: i32,
    pub actual_remaining: usize,
}

#[derive(Serialize, ToSchema)]
pub struct SprintDetail {
    #[serde(flatten)]
    pub sprint: sprints::Model,
    pub task_counts: SprintTaskCounts,
    pub burndown: Vec<BurndownPoint>,
}

fn naive_to_time_date(date: NaiveDate) -> time::Date {
    time::Date::from_calendar_date(
        date.year(),
        time::Month::try_from(date.month() as u8).expect("month"),
        date.day() as u8,
    )
    .expect("valid date")
}

fn validate_date_range(start: NaiveDate, end: NaiveDate) -> Result<(), AppError> {
    if start > end {
        return Err(AppError::BadRequest);
    }
    Ok(())
}

fn parse_sprint_status(value: &str) -> Result<SprintStatus, AppError> {
    match value {
        "planning" => Ok(SprintStatus::Planning),
        "active" => Ok(SprintStatus::Active),
        "completed" => Ok(SprintStatus::Completed),
        _ => Err(AppError::BadRequest),
    }
}

fn time_date_to_naive(date: time::Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
        .expect("valid sprint date")
}

fn end_of_day_utc(date: time::Date) -> chrono::DateTime<chrono::Utc> {
    time_date_to_naive(date)
        .and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
        .and_utc()
}

async fn load_sprint(
    state: &AppState,
    project_id: Uuid,
    id: Uuid,
) -> Result<sprints::Model, AppError> {
    sprints::Entity::find_by_id(id)
        .filter(sprints::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

async fn done_status_ids(state: &AppState, project_id: Uuid) -> Result<HashSet<Uuid>, AppError> {
    Ok(project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .filter(project_statuses::Column::IsDoneState.eq(true))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|s| s.id)
        .collect())
}

fn build_burndown(
    sprint: &sprints::Model,
    sprint_tasks: &[tasks::Model],
    done_statuses: &HashSet<Uuid>,
) -> Vec<BurndownPoint> {
    let today = time::OffsetDateTime::now_utc().date();
    let end_cap = if sprint.end_date < today {
        sprint.end_date
    } else {
        today
    };

    if sprint.start_date > end_cap {
        return Vec::new();
    }

    let start_naive = time_date_to_naive(sprint.start_date);
    let total_at_start = sprint_tasks
        .iter()
        .filter(|t| t.created_at.date_naive() <= start_naive)
        .count();
    let span_days = (sprint.end_date - sprint.start_date).whole_days();
    let mut points = Vec::new();
    let mut cursor = sprint.start_date;

    while cursor <= end_cap {
        let day_offset = (cursor - sprint.start_date).whole_days();
        let ideal_remaining = if span_days <= 0 {
            0
        } else if total_at_start == 0 {
            0
        } else {
            let remaining_ratio = 1.0 - (day_offset as f64 / span_days as f64);
            (total_at_start as f64 * remaining_ratio).round() as i32
        };

        let eod = end_of_day_utc(cursor);
        let actual_remaining = sprint_tasks
            .iter()
            .filter(|t| {
                let created_date = t.created_at.date_naive();
                let created_on_or_before =
                    created_date <= time_date_to_naive(cursor);
                if !created_on_or_before {
                    return false;
                }
                if done_statuses.contains(&t.status_id) {
                    t.completed_at.is_none_or(|completed_at| completed_at > eod)
                } else {
                    true
                }
            })
            .count();

        points.push(BurndownPoint {
            date: cursor,
            ideal_remaining,
            actual_remaining,
        });

        cursor += time::Duration::days(1);
    }

    points
}

async fn build_sprint_detail(
    state: &AppState,
    sprint: sprints::Model,
) -> Result<SprintDetail, AppError> {
    let done_statuses = done_status_ids(state, sprint.project_id).await?;
    let sprint_tasks = tasks::Entity::find()
        .filter(tasks::Column::SprintId.eq(sprint.id))
        .filter(tasks::Column::DeletedAt.is_null())
        .all(&state.db)
        .await?;

    let total = sprint_tasks.len();
    let done = sprint_tasks
        .iter()
        .filter(|t| done_statuses.contains(&t.status_id))
        .count();
    let in_progress = total.saturating_sub(done);

    let burndown = build_burndown(&sprint, &sprint_tasks, &done_statuses);

    Ok(SprintDetail {
        sprint,
        task_counts: SprintTaskCounts {
            total,
            done,
            in_progress,
        },
        burndown,
    })
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Sprints",
    summary = "スプリント一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ListSprintsQuery,
    ),
    responses(
        (status = 200, description = "スプリント一覧", body = [sprints::Model]),
        CrudErrors,
    )
)]
pub async fn list_sprints(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<ListSprintsQuery>,
) -> Result<Json<Vec<sprints::Model>>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadSprint)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let mut query = sprints::Entity::find().filter(sprints::Column::ProjectId.eq(project_id));
    if let Some(ref status) = q.status {
        let status = parse_sprint_status(status)?;
        query = query.filter(sprints::Column::Status.eq(status));
    }

    Ok(Json(query.all(&state.db).await?))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Sprints",
    summary = "スプリント作成",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = CreateSprintRequest,
    responses(
        (status = 201, description = "作成されたスプリント", body = sprints::Model),
        CrudErrors,
    )
)]
pub async fn create_sprint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateSprintRequest>>,
) -> Result<(StatusCode, Json<sprints::Model>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteSprint)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    validate_date_range(payload.start_date, payload.end_date)?;
    let start_date = naive_to_time_date(payload.start_date);
    let end_date = naive_to_time_date(payload.end_date);

    let model = sprints::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set(payload.name),
        goal: Set(payload.goal),
        start_date: Set(start_date),
        end_date: Set(end_date),
        status: Set(SprintStatus::Planning),
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
    tag = "Sprints",
    summary = "スプリント取得（バーンダウン含む）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "スプリントID"),
    ),
    responses(
        (status = 200, description = "スプリント詳細", body = SprintDetail),
        CrudErrors,
    )
)]
pub async fn get_sprint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<SprintDetail>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadSprint)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let sprint = load_sprint(&state, project_id, id).await?;
    Ok(Json(build_sprint_detail(&state, sprint).await?))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Sprints",
    summary = "スプリント更新（planning のみ）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "スプリントID"),
    ),
    request_body = UpdateSprintRequest,
    responses(
        (status = 200, description = "更新後のスプリント", body = sprints::Model),
        CrudErrors,
    )
)]
pub async fn update_sprint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateSprintRequest>>,
) -> Result<Json<sprints::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteSprint)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let txn = state.db.begin().await?;

    let sprint = sprints::Entity::find_by_id(id)
        .filter(sprints::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    if sprint.status != SprintStatus::Planning {
        return Err(AppError::Conflict);
    }

    let start = payload
        .start_date
        .map(naive_to_time_date)
        .unwrap_or(sprint.start_date);
    let end = payload
        .end_date
        .map(naive_to_time_date)
        .unwrap_or(sprint.end_date);
    validate_date_range(
        time_date_to_naive(start),
        time_date_to_naive(end),
    )?;

    let mut active: sprints::ActiveModel = sprint.into();
    if let Some(v) = payload.name {
        active.name = Set(v);
    }
    if payload.clear_goal {
        active.goal = Set(None);
    } else if let Some(v) = payload.goal {
        active.goal = Set(Some(v));
    }
    if let Some(v) = payload.start_date {
        active.start_date = Set(naive_to_time_date(v));
    }
    if let Some(v) = payload.end_date {
        active.end_date = Set(naive_to_time_date(v));
    }
    active.updated_at = Set(chrono::Utc::now());

    let updated = active.update(&txn).await?;
    txn.commit().await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Sprints",
    summary = "スプリント削除（planning のみ）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "スプリントID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_sprint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteSprint)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let txn = state.db.begin().await?;

    let sprint = sprints::Entity::find_by_id(id)
        .filter(sprints::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    if sprint.status != SprintStatus::Planning {
        return Err(AppError::Conflict);
    }

    sprints::Entity::delete_by_id(id).exec(&txn).await?;
    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/start",
    tag = "Sprints",
    summary = "スプリント開始",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "スプリントID"),
    ),
    responses(
        (status = 200, description = "開始後のスプリント", body = sprints::Model),
        CrudErrors,
    )
)]
pub async fn start_sprint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<sprints::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteSprint)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let txn = state.db.begin().await?;

    let sprint = sprints::Entity::find_by_id(id)
        .filter(sprints::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    if sprint.status != SprintStatus::Planning {
        return Err(AppError::Conflict);
    }

    let active_exists = sprints::Entity::find()
        .filter(sprints::Column::ProjectId.eq(project_id))
        .filter(sprints::Column::Status.eq(SprintStatus::Active))
        .one(&txn)
        .await?;
    if active_exists.is_some() {
        return Err(AppError::Conflict);
    }

    let mut active: sprints::ActiveModel = sprint.into();
    active.status = Set(SprintStatus::Active);
    active.updated_at = Set(chrono::Utc::now());
    let updated = match active.update(&txn).await {
        Ok(model) => model,
        Err(e) if is_postgres_unique_violation(&e) => return Err(AppError::Conflict),
        Err(e) => return Err(e.into()),
    };
    txn.commit().await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/complete",
    tag = "Sprints",
    summary = "スプリント完了",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "スプリントID"),
    ),
    request_body = CompleteSprintRequest,
    responses(
        (status = 200, description = "完了後のスプリント", body = sprints::Model),
        CrudErrors,
    )
)]
pub async fn complete_sprint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<CompleteSprintRequest>,
) -> Result<Json<sprints::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteSprint)?;
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    if payload.move_incomplete_to_backlog && payload.move_incomplete_to_sprint_id.is_some() {
        return Err(AppError::BadRequest);
    }

    if let Some(target_id) = payload.move_incomplete_to_sprint_id {
        if target_id == id {
            return Err(AppError::BadRequest);
        }
    }

    let done_statuses = done_status_ids(&state, project_id).await?;
    let txn = state.db.begin().await?;

    let mut sprint_ids = vec![id];
    if let Some(target_id) = payload.move_incomplete_to_sprint_id {
        sprint_ids.push(target_id);
        sprint_ids.sort();
    }
    let locked_sprints = sprints::Entity::find()
        .filter(sprints::Column::ProjectId.eq(project_id))
        .filter(sprints::Column::Id.is_in(sprint_ids))
        .order_by_asc(sprints::Column::Id)
        .lock(LockType::Update)
        .all(&txn)
        .await?;
    let sprint = locked_sprints
        .iter()
        .find(|sprint| sprint.id == id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    if sprint.status != SprintStatus::Active {
        return Err(AppError::Conflict);
    }
    if let Some(target_id) = payload.move_incomplete_to_sprint_id {
        let target = locked_sprints
            .iter()
            .find(|sprint| sprint.id == target_id)
            .ok_or(AppError::NotFound)?;
        if target.status == SprintStatus::Completed {
            return Err(AppError::BadRequest);
        }
    }

    let incomplete = tasks::Entity::find()
        .filter(tasks::Column::SprintId.eq(id))
        .filter(tasks::Column::DeletedAt.is_null())
        .all(&txn)
        .await?
        .into_iter()
        .filter(|t| !done_statuses.contains(&t.status_id))
        .collect::<Vec<_>>();

    let new_sprint_id = if payload.move_incomplete_to_backlog {
        None
    } else if let Some(target_id) = payload.move_incomplete_to_sprint_id {
        Some(target_id)
    } else {
        None
    };

    for task in incomplete {
        let mut active: tasks::ActiveModel = task.into();
        active.sprint_id = Set(new_sprint_id);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&txn).await?;
    }

    let mut active: sprints::ActiveModel = sprint.into();
    active.status = Set(SprintStatus::Completed);
    active.updated_at = Set(chrono::Utc::now());
    let updated = active.update(&txn).await?;

    txn.commit().await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/tasks",
    tag = "Sprints",
    summary = "タスクをスプリントに割り当て",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "スプリントID"),
    ),
    request_body = AssignTasksRequest,
    responses(
        (status = 200, description = "割り当て後のタスク一覧", body = [tasks::Model]),
        CrudErrors,
    )
)]
pub async fn assign_tasks_to_sprint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<AssignTasksRequest>>,
) -> Result<Json<Vec<tasks::Model>>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteSprint)?;
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let unique_ids: Vec<Uuid> = {
        let mut ids = payload.task_ids.clone();
        ids.sort();
        ids.dedup();
        ids
    };

    let txn = state.db.begin().await?;
    let sprint = sprints::Entity::find_by_id(id)
        .filter(sprints::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    if sprint.status == SprintStatus::Completed {
        return Err(AppError::Conflict);
    }

    let found = tasks::Entity::find()
        .filter(tasks::Column::Id.is_in(unique_ids.clone()))
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .all(&txn)
        .await?;

    if found.len() != unique_ids.len() {
        return Err(AppError::NotFound);
    }

    let mut updated = Vec::with_capacity(found.len());
    for task in found {
        let mut active: tasks::ActiveModel = task.into();
        active.sprint_id = Set(Some(id));
        active.updated_at = Set(chrono::Utc::now());
        updated.push(active.update(&txn).await?);
    }

    txn.commit().await?;
    Ok(Json(updated))
}

use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Path, State},
};
use axum_extra::extract::Query;
use chrono::NaiveDate;
use common::cursor::{decode_cursor, encode_cursor};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, FromQueryResult, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QueryResult, QuerySelect, QueryTrait, Value,
    prelude::Uuid, sea_query::Expr,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth_helpers::{is_tenant_owner, visible_project_ids},
    error::AppError,
    extractors::AuthUser,
    openapi::CrudErrors,
};
use entity::{
    milestones, project_statuses, projects, scopes::Scope, sprints, task_assignees, tasks, users,
};
use payload::{
    my_tasks::{MyTaskProjectInfo, MyTaskStatusInfo},
    schedule::*,
    users::UserSummary,
};

const MAX_RANGE_DAYS: i64 = 92;
const DEFAULT_LIMIT: u64 = 200;
const MAX_LIMIT: u64 = 500;

// 開始予定日・Soft 締切・Hard 締切の日付のうち、最も早い日を始点・最も遅い日を終点にする。$1 はタイムゾーン。
// tasks の CHECK で Soft ≤ Hard なので、締切の早い方は Soft → Hard、遅い方は Hard → Soft で取れる。
// 開始予定日が締切より後のタスクも許されているため、逆転したまま重なり判定すると両端の日から落ちる。
// LEAST / GREATEST は NULL を無視するので、片方しか無ければ両端ともその日になる
const END_DATE: &str = "GREATEST(tasks.planned_start_on, (COALESCE(tasks.hard_deadline, tasks.soft_deadline) AT TIME ZONE $1)::date)";
const START_DATE: &str = "LEAST(tasks.planned_start_on, (COALESCE(tasks.soft_deadline, tasks.hard_deadline) AT TIME ZONE $1)::date)";
const NOT_DONE: &str = "NOT EXISTS (SELECT 1 FROM project_statuses s WHERE s.id = tasks.status_id AND s.is_done_state)";
// 作業記録の合計。担当者以外の記録も含め、`time-logs/summary` の `actual_minutes` と同じ定義にする
const LOGGED_MINUTES: &str =
    "(SELECT COALESCE(SUM(l.logged_minutes), 0) FROM time_logs l WHERE l.task_id = tasks.id)::int4";

#[derive(Serialize, Deserialize)]
struct ScheduleCursor {
    start: NaiveDate,
    id: Uuid,
}

#[axum::debug_handler]
#[utoipa::path(
    get, path = "/{tenant_id}/schedule", tag = "Schedule",
    summary = "期間に重なるタスク・スプリント・マイルストーン",
    params(("tenant_id" = Uuid, Path, description = "テナントID"), ScheduleQuery),
    responses((status = 200, description = "スケジュール", body = ScheduleResponse), CrudErrors)
)]
pub async fn get_schedule(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Query(q): Query<ScheduleQuery>,
) -> Result<Json<ScheduleResponse>, AppError> {
    auth.require_scope(Scope::ReadTask)?;
    let guest_ids = auth
        .ensure_tenant_access_or_guest_scope(&state, tenant_id)
        .await?;
    if q.from > q.to || (q.to - q.from).num_days() > MAX_RANGE_DAYS {
        return Err(AppError::BadRequest);
    }
    let timezone = q.timezone.as_deref().unwrap_or("UTC");
    if timezone.len() > 100 || !service::timezone::exists(&state.db, timezone).await? {
        return Err(AppError::BadRequest);
    }
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(AppError::BadRequest);
    }
    let cursor = q
        .cursor
        .as_deref()
        .map(decode_cursor::<ScheduleCursor>)
        .transpose()?;

    let project_map = schedule_projects(&state, &auth, tenant_id, guest_ids, &q.project_id).await?;
    let project_ids: Vec<Uuid> = project_map.keys().copied().collect();

    let tz = || Value::from(timezone.to_owned());
    let mut query = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.is_in(project_ids.clone()))
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(tasks::Column::IsArchived.eq(false))
        .filter(Expr::cust_with_values(
            if q.include_overdue == Some(true) {
                // 終点が from より前でも、山積みに積める（Soft 締切・開始予定日・見積がある）未完了の
                // ものは締切超過として含める。積めないものまで返すと締切超過の backlog を毎回全部取る
                format!(
                    "{START_DATE} <= $2 AND ({END_DATE} >= $3 OR (tasks.soft_deadline IS NOT NULL \
                     AND tasks.planned_start_on IS NOT NULL AND tasks.estimated_minutes IS NOT NULL \
                     AND {NOT_DONE}))"
                )
            } else {
                format!("{START_DATE} <= $2 AND {END_DATE} >= $3")
            },
            [tz(), q.to.into(), q.from.into()],
        ))
        .column_as(Expr::cust_with_values(START_DATE, [tz()]), "schedule_start")
        .column_as(Expr::cust(LOGGED_MINUTES), "logged_minutes");
    if !q.user_id.is_empty() {
        query = query.filter(Expr::cust_with_values(
            "EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = tasks.id AND a.user_id = ANY($1))",
            [Value::from(q.user_id.clone())],
        ));
    }
    if q.include_done == Some(false) {
        query = query.filter(Expr::cust(NOT_DONE));
    }
    if let Some(c) = &cursor {
        query = query.filter(Expr::cust_with_values(
            format!("({START_DATE}, tasks.id) > ($2, $3)"),
            [tz(), c.start.into(), c.id.into()],
        ));
    }
    let statement = query
        .order_by(Expr::cust_with_values(START_DATE, [tz()]), Order::Asc)
        .order_by_asc(tasks::Column::Id)
        .limit(limit + 1)
        .build(DatabaseBackend::Postgres);
    let mut rows = state.db.query_all_raw(statement).await?;
    let next_cursor = if rows.len() as u64 > limit {
        rows.truncate(limit as usize);
        let last = rows.last().ok_or(AppError::NotFound)?;
        Some(encode_cursor(&ScheduleCursor {
            start: last.try_get("", "schedule_start")?,
            id: last.try_get("", "id")?,
        }))
    } else {
        None
    };
    let (items, members) = schedule_tasks(&state, &rows, &project_map).await?;

    let sprints = sprints::Entity::find()
        .filter(sprints::Column::ProjectId.is_in(project_ids.clone()))
        .filter(sprints::Column::StartDate.lte(q.to))
        .filter(sprints::Column::EndDate.gte(q.from))
        .order_by_asc(sprints::Column::StartDate)
        .order_by_asc(sprints::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    let milestones = milestones::Entity::find()
        .filter(milestones::Column::ProjectId.is_in(project_ids))
        .filter(milestones::Column::DueDate.between(q.from, q.to))
        .order_by_asc(milestones::Column::DueDate)
        .order_by_asc(milestones::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(ScheduleResponse {
        range: ScheduleRange {
            from: q.from,
            to: q.to,
            timezone: timezone.to_owned(),
        },
        tasks: items,
        sprints,
        milestones,
        members,
        next_cursor,
    }))
}

#[derive(Serialize, Deserialize)]
struct UnscheduledCursor {
    id: Uuid,
}

#[axum::debug_handler]
#[utoipa::path(
    get, path = "/{tenant_id}/schedule/unscheduled", tag = "Schedule",
    summary = "開始予定日も締切も持たないタスク",
    params(("tenant_id" = Uuid, Path, description = "テナントID"), UnscheduledQuery),
    responses((status = 200, description = "期間の無いタスク", body = UnscheduledResponse), CrudErrors)
)]
pub async fn get_unscheduled(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Query(q): Query<UnscheduledQuery>,
) -> Result<Json<UnscheduledResponse>, AppError> {
    auth.require_scope(Scope::ReadTask)?;
    let guest_ids = auth
        .ensure_tenant_access_or_guest_scope(&state, tenant_id)
        .await?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(AppError::BadRequest);
    }
    let cursor = q
        .cursor
        .as_deref()
        .map(decode_cursor::<UnscheduledCursor>)
        .transpose()?;

    // 見えるプロジェクトの決め方は予定と同じ口を通す。ガントの棒と期間なしの数で範囲がずれないようにする
    let project_map = schedule_projects(&state, &auth, tenant_id, guest_ids, &q.project_id).await?;
    let project_ids: Vec<Uuid> = project_map.keys().copied().collect();

    let mut query = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.is_in(project_ids))
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(tasks::Column::IsArchived.eq(false))
        .filter(tasks::Column::PlannedStartOn.is_null())
        .filter(tasks::Column::SoftDeadline.is_null())
        .filter(tasks::Column::HardDeadline.is_null());
    if !q.user_id.is_empty() {
        query = query.filter(Expr::cust_with_values(
            "EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = tasks.id AND a.user_id = ANY($1))",
            [Value::from(q.user_id.clone())],
        ));
    }
    // 予定と違い既定で完了を除く。済んだ課題に日付を入れる手間は要らないため
    if q.include_done != Some(true) {
        query = query.filter(Expr::cust(NOT_DONE));
    }
    let total = query.clone().count(&state.db).await?;
    if let Some(c) = &cursor {
        query = query.filter(tasks::Column::Id.gt(c.id));
    }
    let statement = query
        .column_as(Expr::cust(LOGGED_MINUTES), "logged_minutes")
        .order_by_asc(tasks::Column::Id)
        .limit(limit + 1)
        .build(DatabaseBackend::Postgres);
    let mut rows = state.db.query_all_raw(statement).await?;
    let next_cursor = if rows.len() as u64 > limit {
        rows.truncate(limit as usize);
        let last = rows.last().ok_or(AppError::NotFound)?;
        Some(encode_cursor(&UnscheduledCursor {
            id: last.try_get("", "id")?,
        }))
    } else {
        None
    };
    let (items, members) = schedule_tasks(&state, &rows, &project_map).await?;

    Ok(Json(UnscheduledResponse {
        total,
        tasks: items,
        members,
        next_cursor,
    }))
}

/// 予定に出してよいプロジェクト。テナントの個人でないプロジェクトのうち、オーナーは全部、
/// 客分は明示のメンバーのもの、それ以外は見えるもの。`requested` が空でなければその中に絞る
async fn schedule_projects(
    state: &AppState,
    auth: &AuthUser,
    tenant_id: Uuid,
    guest_ids: Option<HashSet<Uuid>>,
    requested: &[Uuid],
) -> Result<HashMap<Uuid, projects::Model>, AppError> {
    let candidates = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .filter(projects::Column::IsPersonal.eq(false))
        .all(&state.db)
        .await?;
    let candidate_ids: Vec<Uuid> = candidates.iter().map(|p| p.id).collect();
    // オーナーは全プロジェクトに入れる（dashboard / プロジェクト一覧と同じ）
    let visible = if let Some(ids) = guest_ids {
        ids
    } else if is_tenant_owner(&state.db, tenant_id, auth.user_id).await? {
        candidate_ids.into_iter().collect()
    } else {
        visible_project_ids(&state.db, candidate_ids, auth.user_id).await?
    };
    let requested: HashSet<Uuid> = requested.iter().copied().collect();
    Ok(candidates
        .into_iter()
        .filter(|p| visible.contains(&p.id) && (requested.is_empty() || requested.contains(&p.id)))
        .map(|p| (p.id, p))
        .collect())
}

/// 引いた行を予定のタスクと、その担当者に写す
async fn schedule_tasks(
    state: &AppState,
    rows: &[QueryResult],
    project_map: &HashMap<Uuid, projects::Model>,
) -> Result<(Vec<ScheduleTask>, Vec<UserSummary>), AppError> {
    let page = rows
        .iter()
        .map(|row| {
            Ok::<_, sea_orm::DbErr>((
                tasks::Model::from_query_result(row, "")?,
                row.try_get::<i32>("", "logged_minutes")?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let task_ids: Vec<Uuid> = page.iter().map(|(t, _)| t.id).collect();
    let statuses: HashMap<Uuid, project_statuses::Model> = project_statuses::Entity::find()
        .filter(project_statuses::Column::Id.is_in(page.iter().map(|(t, _)| t.status_id)))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|s| (s.id, s))
        .collect();
    let mut assignees: HashMap<Uuid, Vec<ScheduleAssignee>> = HashMap::new();
    let mut member_ids = Vec::new();
    for a in task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.is_in(task_ids))
        .order_by_asc(task_assignees::Column::AssignedAt)
        .order_by_asc(task_assignees::Column::Id)
        .all(&state.db)
        .await?
    {
        member_ids.push(a.user_id);
        assignees
            .entry(a.task_id)
            .or_default()
            .push(ScheduleAssignee {
                user_id: a.user_id,
                role: a.role,
            });
    }
    let members = users::Entity::find()
        .filter(users::Column::Id.is_in(member_ids))
        .order_by_asc(users::Column::Username)
        .order_by_asc(users::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    let mut items = Vec::with_capacity(page.len());
    for (task, logged_minutes) in page {
        let project = project_map
            .get(&task.project_id)
            .ok_or(AppError::NotFound)?;
        let status = statuses.get(&task.status_id).ok_or(AppError::NotFound)?;
        items.push(ScheduleTask {
            id: task.id,
            seq_id: task.seq_id,
            seq_key: format!("{}-{}", project.key, task.seq_id),
            title: task.title,
            project: MyTaskProjectInfo {
                id: project.id,
                name: project.name.clone(),
                key: project.key.clone(),
                is_personal: project.is_personal,
            },
            status: MyTaskStatusInfo {
                id: status.id,
                name: status.name.clone(),
                color: status.color.clone(),
                is_done_state: status.is_done_state,
            },
            priority: task.priority.into(),
            progress_pct: task.progress_pct,
            planned_start_on: task.planned_start_on,
            soft_deadline: task.soft_deadline.map(|d| d.to_utc()),
            hard_deadline: task.hard_deadline.map(|d| d.to_utc()),
            estimated_minutes: task.estimated_minutes,
            logged_minutes,
            assignees: assignees.remove(&task.id).unwrap_or_default(),
            sprint_id: task.sprint_id,
            milestone_id: task.milestone_id,
        });
    }
    Ok((items, members))
}

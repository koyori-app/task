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
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, Value, prelude::Uuid, sea_query::Expr,
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
};

const MAX_RANGE_DAYS: i64 = 92;
const DEFAULT_LIMIT: u64 = 200;
const MAX_LIMIT: u64 = 500;

// 開始予定日と締切（Soft → Hard）の日付のうち、早い方を始点・遅い方を終点にする。$1 はタイムゾーン。
// 開始予定日が締切より後のタスクも許されているため、逆転したまま重なり判定すると両端の日から落ちる。
// LEAST / GREATEST は NULL を無視するので、片方しか無ければ両端ともその日になる
const END_DATE: &str = "GREATEST(tasks.planned_start_on, (COALESCE(tasks.soft_deadline, tasks.hard_deadline) AT TIME ZONE $1)::date)";
const START_DATE: &str = "LEAST(tasks.planned_start_on, (COALESCE(tasks.soft_deadline, tasks.hard_deadline) AT TIME ZONE $1)::date)";

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
    let requested: HashSet<Uuid> = q.project_id.iter().copied().collect();
    let project_map: HashMap<Uuid, projects::Model> = candidates
        .into_iter()
        .filter(|p| visible.contains(&p.id) && (requested.is_empty() || requested.contains(&p.id)))
        .map(|p| (p.id, p))
        .collect();
    let project_ids: Vec<Uuid> = project_map.keys().copied().collect();

    let tz = || Value::from(timezone.to_owned());
    let mut query = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.is_in(project_ids.clone()))
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(tasks::Column::IsArchived.eq(false))
        .filter(Expr::cust_with_values(
            format!("{START_DATE} <= $2 AND {END_DATE} >= $3"),
            [tz(), q.to.into(), q.from.into()],
        ))
        .column_as(Expr::cust_with_values(START_DATE, [tz()]), "schedule_start");
    if !q.user_id.is_empty() {
        query = query.filter(Expr::cust_with_values(
            "EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = tasks.id AND a.user_id = ANY($1))",
            [Value::from(q.user_id.clone())],
        ));
    }
    if q.include_done == Some(false) {
        query = query.filter(Expr::cust(
            "NOT EXISTS (SELECT 1 FROM project_statuses s WHERE s.id = tasks.status_id AND s.is_done_state)",
        ));
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
    let page = rows
        .iter()
        .map(|row| tasks::Model::from_query_result(row, ""))
        .collect::<Result<Vec<_>, _>>()?;

    let task_ids: Vec<Uuid> = page.iter().map(|t| t.id).collect();
    let statuses: HashMap<Uuid, project_statuses::Model> = project_statuses::Entity::find()
        .filter(project_statuses::Column::Id.is_in(page.iter().map(|t| t.status_id)))
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
    for task in page {
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
            priority: task.priority,
            progress_pct: task.progress_pct,
            planned_start_on: task.planned_start_on,
            soft_deadline: task.soft_deadline.map(|d| d.to_utc()),
            hard_deadline: task.hard_deadline.map(|d| d.to_utc()),
            estimated_minutes: task.estimated_minutes,
            assignees: assignees.remove(&task.id).unwrap_or_default(),
            sprint_id: task.sprint_id,
            milestone_id: task.milestone_id,
        });
    }

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

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::Utc;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseBackend, EntityTrait, FromQueryResult,
    QueryFilter, QueryOrder, Statement, Value, prelude::Uuid,
};

use crate::{
    AppState,
    auth_helpers::{is_tenant_owner, visible_project_ids},
    error::AppError,
    extractors::AuthUser,
    openapi::CrudErrors,
};
use entity::{project_statuses, projects, scopes::Scope, tasks};
use payload::{
    dashboard::*,
    task_comments::{ActivityItem, ActivityUser},
};

// Every aggregate and preview uses the same authorized project set. Counts are independent
// of the task page size; no full task/history list is loaded to calculate the dashboard.
const SCOPED: &str = r#"
WITH scoped AS (
    SELECT t.*, s.is_done_state,
           (COALESCE(t.soft_deadline, t.hard_deadline) AT TIME ZONE 'UTC')::date AS due_date
    FROM tasks t JOIN project_statuses s ON s.id = t.status_id
    WHERE t.project_id = ANY($1) AND t.deleted_at IS NULL AND NOT t.is_archived
), calendar AS (
    SELECT (now() AT TIME ZONE $3)::date AS today,
           date_trunc('week', now() AT TIME ZONE $3)::date AS week_start
), mine AS (
    SELECT t.* FROM scoped t
    WHERE EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id AND a.user_id = $2)
)
"#;

#[axum::debug_handler]
#[utoipa::path(
    get, path = "/dashboard", tag = "My Tasks", summary = "ホームのタスク・進捗・最近の更新",
    params(("tenant_id" = Uuid, Path, description = "テナントID"), DashboardQuery),
    responses((status = 200, description = "ダッシュボード", body = DashboardResponse), CrudErrors)
)]
pub async fn get_dashboard(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Query(q): Query<DashboardQuery>,
) -> Result<Json<DashboardResponse>, AppError> {
    auth.require_scope(Scope::ReadTask)?;
    auth.require_scope(Scope::ReadProject)?;
    let guest_ids = auth
        .ensure_tenant_access_or_guest_scope(&state, tenant_id)
        .await?;
    let filter = q.filter.as_deref().unwrap_or("today");
    let predicate = match filter {
        "today" => "due_date = c.today",
        "week" => "due_date >= c.week_start AND due_date < c.week_start + 7",
        "overdue" => "due_date < c.today",
        "all" => "TRUE",
        _ => return Err(AppError::BadRequest),
    };
    let timezone = q.timezone.as_deref().unwrap_or("UTC");
    if timezone.len() > 100
        || state
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT name FROM pg_timezone_names WHERE name = $1",
                [timezone.into()],
            ))
            .await?
            .is_none()
    {
        return Err(AppError::BadRequest);
    }
    let limit = q.limit.unwrap_or(5).clamp(1, 50);
    let offset = i64::try_from(q.offset.unwrap_or(0)).map_err(|_| AppError::BadRequest)?;
    let candidates = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .filter(
            Condition::any()
                .add(projects::Column::IsPersonal.eq(false))
                .add(projects::Column::PersonalOwnerId.eq(auth.user_id)),
        )
        .order_by_asc(projects::Column::Name)
        .order_by_asc(projects::Column::Id)
        .all(&state.db)
        .await?;
    let ids = candidates.iter().map(|p| p.id).collect::<Vec<_>>();
    let visible_ids = if let Some(ids) = guest_ids {
        ids
    } else if is_tenant_owner(&state.db, tenant_id, auth.user_id).await? {
        ids.into_iter().collect()
    } else {
        visible_project_ids(&state.db, ids, auth.user_id).await?
    };
    let projects: Vec<_> = candidates
        .into_iter()
        .filter(|p| visible_ids.contains(&p.id))
        .collect();
    let project_ids: Vec<Uuid> = projects.iter().map(|p| p.id).collect();
    let values: Vec<Value> = vec![
        project_ids.clone().into(),
        auth.user_id.into(),
        timezone.into(),
    ];
    let sql = |suffix: &str| {
        Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!("{SCOPED}{suffix}"),
            values.clone(),
        )
    };
    // Deadlines are calendar dates stored at UTC midnight, as in the task date inputs.
    // Completion timestamps, in contrast, are converted to the viewer's timezone.
    let row = state.db.query_one_raw(sql(r#"
SELECT count(*) FILTER (WHERE NOT is_done_state AND due_date = c.today) AS today,
       count(*) FILTER (WHERE NOT is_done_state AND due_date >= c.week_start AND due_date < c.week_start + 7) AS week,
       count(*) FILTER (WHERE NOT is_done_state AND due_date < c.today) AS overdue,
       count(*) FILTER (WHERE NOT is_done_state) AS open,
       count(*) FILTER (WHERE is_done_state AND (completed_at AT TIME ZONE $3)::date >= c.week_start AND (completed_at AT TIME ZONE $3)::date < c.week_start + 7) AS completed_week
FROM mine CROSS JOIN calendar c
"#)).await?.ok_or(AppError::NotFound)?;
    let counts = DashboardCounts {
        today: row.try_get("", "today")?,
        week: row.try_get("", "week")?,
        overdue: row.try_get("", "overdue")?,
        open: row.try_get("", "open")?,
        completed_week: row.try_get("", "completed_week")?,
    };
    let days = state.db.query_all_raw(sql(r#"
SELECT (c.week_start + d.day)::text AS date, count(t.id) AS count
FROM calendar c CROSS JOIN generate_series(0, 6) d(day)
LEFT JOIN mine t ON t.is_done_state AND (t.completed_at AT TIME ZONE $3)::date = c.week_start + d.day
GROUP BY c.week_start, d.day ORDER BY d.day
"#)).await?.into_iter().map(|row| Ok(DashboardDay {date:row.try_get("", "date")?, count:row.try_get("", "count")?})).collect::<Result<Vec<_>, sea_orm::DbErr>>()?;
    let statuses = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.is_in(project_ids))
        .all(&state.db)
        .await?;
    let statuses: HashMap<_, _> = statuses.into_iter().map(|s| (s.id, s)).collect();
    let project_map: HashMap<_, _> = projects.iter().map(|p| (p.id, p)).collect();
    let mut page_values = values.clone();
    page_values.extend([Value::from(limit as i64), Value::from(offset)]);
    let rows = state.db.query_all_raw(Statement::from_sql_and_values(DatabaseBackend::Postgres, format!("{SCOPED} SELECT t.* FROM mine t CROSS JOIN calendar c WHERE NOT is_done_state AND {predicate} ORDER BY due_date ASC NULLS LAST, id ASC LIMIT $4 OFFSET $5"), page_values)).await?;
    let mut task_items = Vec::with_capacity(rows.len());
    for row in rows {
        let task = tasks::Model::from_query_result(&row, "")?;
        let project = project_map
            .get(&task.project_id)
            .ok_or(AppError::NotFound)?;
        let status = statuses.get(&task.status_id).ok_or(AppError::NotFound)?;
        let done_status_id = statuses
            .values()
            .find(|s| s.project_id == task.project_id && s.is_done_state && s.is_default_done)
            .map(|s| s.id);
        task_items.push(DashboardTask {
            task: super::my_tasks::build_my_task_item(task, project, status),
            done_status_id,
        });
    }
    let project_counts = state.db.query_all_raw(sql("SELECT project_id, count(*) AS total, count(*) FILTER (WHERE is_done_state) AS completed FROM scoped GROUP BY project_id")).await?;
    let project_counts = project_counts
        .into_iter()
        .map(|r| {
            Ok((
                r.try_get::<Uuid>("", "project_id")?,
                (
                    r.try_get::<i64>("", "total")?,
                    r.try_get::<i64>("", "completed")?,
                ),
            ))
        })
        .collect::<Result<HashMap<_, _>, sea_orm::DbErr>>()?;
    let recent = state
        .db
        .query_all_raw(sql(r#"
SELECT a.id, a.event_type, a.payload, a.created_at, a.user_id, u.username,
       t.title, t.seq_id, p.key, p.name
FROM task_activities a JOIN scoped t ON t.id = a.task_id
JOIN projects p ON p.id = t.project_id LEFT JOIN users u ON u.id = a.user_id
ORDER BY a.created_at DESC, a.id DESC LIMIT 3
"#))
        .await?;
    let mut activities = Vec::with_capacity(recent.len());
    for row in recent {
        let user_id: Option<Uuid> = row.try_get("", "user_id")?;
        let username: Option<String> = row.try_get("", "username")?;
        activities.push(DashboardActivity {
            activity: ActivityItem {
                id: row.try_get("", "id")?,
                event_type: row.try_get("", "event_type")?,
                payload: row.try_get("", "payload")?,
                created_at: row
                    .try_get::<chrono::DateTime<chrono::FixedOffset>>("", "created_at")?
                    .with_timezone(&Utc),
                user: user_id.map(|id| ActivityUser {
                    id,
                    name: username.unwrap_or_else(|| "unknown".into()),
                }),
            },
            task_title: row.try_get("", "title")?,
            task_seq_id: row.try_get("", "seq_id")?,
            project_key: row.try_get("", "key")?,
            project_name: row.try_get("", "name")?,
        });
    }
    let total = match filter {
        "today" => counts.today,
        "week" => counts.week,
        "overdue" => counts.overdue,
        _ => counts.open,
    };
    Ok(Json(DashboardResponse {
        counts,
        days,
        tasks: task_items,
        total,
        activities,
        projects: projects
            .into_iter()
            .map(|p| {
                let (total, completed) = project_counts.get(&p.id).copied().unwrap_or_default();
                DashboardProject {
                    project: p.into(),
                    total,
                    completed,
                }
            })
            .collect(),
    }))
}

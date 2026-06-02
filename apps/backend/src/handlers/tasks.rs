use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use axum_valid::Valid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait,
    EntityTrait, IsolationLevel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait, prelude::Uuid,
};
use sea_orm::sea_query::{Expr, LockType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth_helpers::{is_tenant_owner, require_member_or_owner};
use crate::entities::{
    labels, milestones, project_statuses, project_task_counters, task_assignees, task_labels,
    task_relations, tasks,
};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::AppState;

// ─── Task lookup (UUID or KEY-N) ─────────────────────────────────────────

async fn resolve_task(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
    id_str: &str,
) -> Result<tasks::Model, AppError> {
    if let Ok(uuid) = id_str.parse::<Uuid>() {
        return tasks::Entity::find_by_id(uuid)
            .filter(tasks::Column::ProjectId.eq(project_id))
            .filter(tasks::Column::DeletedAt.is_null())
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound);
    }
    let project = crate::entities::projects::Entity::find_by_id(project_id)
        .filter(crate::entities::projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let prefix = format!("{}-", project.key);
    if let Some(n_str) = id_str.strip_prefix(&prefix) {
        if let Ok(seq) = n_str.parse::<i32>() {
            return tasks::Entity::find()
                .filter(tasks::Column::ProjectId.eq(project_id))
                .filter(tasks::Column::SeqId.eq(seq))
                .filter(tasks::Column::DeletedAt.is_null())
                .one(&state.db)
                .await?
                .ok_or(AppError::NotFound);
        }
    }
    Err(AppError::NotFound)
}

// ─── Seq ID counter ──────────────────────────────────────────────────────

async fn next_seq_id(db: &sea_orm::DatabaseTransaction, project_id: Uuid) -> Result<i32, AppError> {
    let existing = project_task_counters::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(db)
        .await?;
    Ok(match existing {
        Some(c) => {
            let new_seq = c.last_seq + 1;
            let mut active: project_task_counters::ActiveModel = c.into();
            active.last_seq = Set(new_seq);
            active.update(db).await?.last_seq
        }
        None => {
            project_task_counters::ActiveModel {
                project_id: Set(project_id),
                last_seq: Set(1),
            }
            .insert(db)
            .await?
            .last_seq
        }
    })
}

// ─── BFS cycle detection ─────────────────────────────────────────────────

async fn would_create_cycle<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    blocker: Uuid,
    blocked: Uuid,
) -> Result<bool, AppError> {
    let project_task_ids: HashSet<Uuid> = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .map(|t| t.id)
        .collect();
    let task_id_vec: Vec<Uuid> = project_task_ids.iter().copied().collect();
    if task_id_vec.is_empty() {
        return Ok(false);
    }
    let all_rels = task_relations::Entity::find()
        .filter(
            Condition::any()
                .add(task_relations::Column::BlockerTaskId.is_in(task_id_vec.clone()))
                .add(task_relations::Column::BlockedTaskId.is_in(task_id_vec)),
        )
        .all(db)
        .await?;
    let mut graph: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for rel in &all_rels {
        if project_task_ids.contains(&rel.blocker_task_id)
            && project_task_ids.contains(&rel.blocked_task_id)
        {
            graph
                .entry(rel.blocker_task_id)
                .or_default()
                .push(rel.blocked_task_id);
        }
    }
    let mut visited: HashSet<Uuid> = HashSet::new();
    let mut queue: VecDeque<Uuid> = VecDeque::new();
    queue.push_back(blocked);
    while let Some(cur) = queue.pop_front() {
        if cur == blocker {
            return Ok(true);
        }
        if visited.insert(cur) {
            if let Some(nexts) = graph.get(&cur) {
                queue.extend(nexts);
            }
        }
    }
    Ok(false)
}

// ─── Parent hierarchy cycle detection ────────────────────────────────────

/// `ancestor_id` が `descendant_id` の祖先か（parent_task_id リンクを上方向に
/// 辿って到達するか）を判定する。`ancestor_id == descendant_id` の場合も true。
/// parent_task_id に循環を作る更新を防ぐために使用する。
async fn is_ancestor_of<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    ancestor_id: Uuid,
    descendant_id: Uuid,
) -> Result<bool, AppError> {
    if ancestor_id == descendant_id {
        return Ok(true);
    }
    let mut visited: HashSet<Uuid> = HashSet::new();
    let mut current = descendant_id;
    loop {
        if !visited.insert(current) {
            return Ok(false);
        }
        let node = tasks::Entity::find_by_id(current)
            .filter(tasks::Column::ProjectId.eq(project_id))
            .one(db)
            .await?;
        match node.and_then(|t| t.parent_task_id) {
            None => return Ok(false),
            Some(parent_id) => {
                if parent_id == ancestor_id {
                    return Ok(true);
                }
                current = parent_id;
            }
        }
    }
}

// ─── DTOs ────────────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct AssigneeInput {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateTaskRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: String,
    pub description: Option<String>,
    #[schema(value_type = String, format = "uuid")]
    pub status_id: Uuid,
    pub priority: Option<tasks::TaskPriority>,
    #[validate(range(min = 0, max = 100))]
    pub progress_pct: Option<i16>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_task_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub milestone_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub soft_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub hard_deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub estimated_minutes: Option<i32>,
    #[serde(default)]
    pub assignees: Vec<AssigneeInput>,
    #[serde(default)]
    pub label_ids: Vec<Uuid>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateTaskRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub clear_description: bool,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub status_id: Option<Uuid>,
    pub priority: Option<tasks::TaskPriority>,
    #[validate(range(min = 0, max = 100))]
    pub progress_pct: Option<i16>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_task_id: Option<Uuid>,
    #[serde(default)]
    pub clear_parent_task_id: bool,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub milestone_id: Option<Uuid>,
    #[serde(default)]
    pub clear_milestone_id: bool,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub soft_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub clear_soft_deadline: bool,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub hard_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub clear_hard_deadline: bool,
    pub estimated_minutes: Option<i32>,
    #[serde(default)]
    pub clear_estimated_minutes: bool,
    pub is_archived: Option<bool>,
}

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct ListTasksQuery {
    pub status_id: Option<Uuid>,
    pub priority: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub milestone_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    #[serde(default)]
    pub is_archived: bool,
    pub sort: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 {
    50
}

fn parse_task_priority(value: &str) -> Result<tasks::TaskPriority, AppError> {
    match value {
        "critical_fire" => Ok(tasks::TaskPriority::CriticalFire),
        "critical" => Ok(tasks::TaskPriority::Critical),
        "high" => Ok(tasks::TaskPriority::High),
        "medium" => Ok(tasks::TaskPriority::Medium),
        "low" => Ok(tasks::TaskPriority::Low),
        "trivial" => Ok(tasks::TaskPriority::Trivial),
        _ => Err(AppError::BadRequest),
    }
}

#[derive(Serialize, ToSchema)]
pub struct TaskListResponse {
    pub tasks: Vec<tasks::Model>,
    pub total: u64,
}

// ─── Tasks ───────────────────────────────────────────────────────────────

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Tasks",
    summary = "タスク一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ListTasksQuery,
    ),
    responses(
        (status = 200, description = "タスク一覧", body = TaskListResponse),
        CrudErrors,
    )
)]
pub async fn list_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<ListTasksQuery>,
) -> Result<Json<TaskListResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let mut query = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(tasks::Column::IsArchived.eq(q.is_archived));

    if let Some(sid) = q.status_id {
        query = query.filter(tasks::Column::StatusId.eq(sid));
    }
    if let Some(ref priority) = q.priority {
        let priority = parse_task_priority(priority)?;
        query = query.filter(tasks::Column::Priority.eq(priority));
    }
    if let Some(mid) = q.milestone_id {
        query = query.filter(tasks::Column::MilestoneId.eq(mid));
    }
    if let Some(pid) = q.parent_task_id {
        query = query.filter(tasks::Column::ParentTaskId.eq(pid));
    }
    if let Some(uid) = q.assignee_id {
        query = query.filter(Expr::cust_with_values(
            "EXISTS (SELECT 1 FROM task_assignees WHERE task_assignees.task_id = tasks.id AND task_assignees.user_id = $1)",
            vec![sea_orm::Value::from(uid)],
        ));
    }

    query = match q.sort.as_deref().unwrap_or("created_at_desc") {
        "priority_asc" => query.order_by_asc(tasks::Column::Priority),
        "deadline_asc" => query.order_by_asc(tasks::Column::SoftDeadline),
        _ => query.order_by_desc(tasks::Column::CreatedAt),
    };

    let limit = std::cmp::min(q.limit, 200);
    let total = query.clone().count(&state.db).await?;
    let tasks_page = query
        .offset(q.offset)
        .limit(limit)
        .all(&state.db)
        .await?;
    Ok(Json(TaskListResponse {
        tasks: tasks_page,
        total,
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Tasks",
    summary = "タスク作成",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = CreateTaskRequest,
    responses(
        (status = 201, description = "作成されたタスク", body = tasks::Model),
        CrudErrors,
    )
)]
pub async fn create_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateTaskRequest>>,
) -> Result<(StatusCode, Json<tasks::Model>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    project_statuses::Entity::find_by_id(payload.status_id)
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let txn = state.db.begin().await?;

    // parent_task_id / milestone_id が同一プロジェクトに属することを検証
    if let Some(pid) = payload.parent_task_id {
        tasks::Entity::find_by_id(pid)
            .filter(tasks::Column::ProjectId.eq(project_id))
            .filter(tasks::Column::DeletedAt.is_null())
            .one(&txn)
            .await?
            .ok_or(AppError::NotFound)?;
    }
    if let Some(mid) = payload.milestone_id {
        milestones::Entity::find_by_id(mid)
            .filter(milestones::Column::ProjectId.eq(project_id))
            .one(&txn)
            .await?
            .ok_or(AppError::NotFound)?;
    }

    let seq_id = next_seq_id(&txn, project_id).await?;

    let model = tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        seq_id: Set(seq_id),
        title: Set(payload.title),
        description: Set(payload.description),
        status_id: Set(payload.status_id),
        priority: Set(payload.priority.unwrap_or(tasks::TaskPriority::Medium)),
        progress_pct: Set(payload.progress_pct.unwrap_or(0)),
        parent_task_id: Set(payload.parent_task_id),
        milestone_id: Set(payload.milestone_id),
        soft_deadline: Set(payload.soft_deadline),
        hard_deadline: Set(payload.hard_deadline),
        estimated_minutes: Set(payload.estimated_minutes),
        is_archived: Set(false),
        created_by: Set(auth.user_id),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        deleted_at: Set(None),
    }
    .insert(&txn)
    .await?;

    for a in &payload.assignees {
        require_member_or_owner(&state, tenant_id, project_id, a.user_id).await?;
        task_assignees::ActiveModel {
            id: Set(Uuid::new_v4()),
            task_id: Set(model.id),
            user_id: Set(a.user_id),
            role: Set(a.role.clone()),
            assigned_at: Set(chrono::Utc::now()),
        }
        .insert(&txn)
        .await?;
    }
    if !payload.label_ids.is_empty() {
        let labels_in_project = labels::Entity::find()
            .filter(labels::Column::Id.is_in(payload.label_ids.clone()))
            .filter(labels::Column::ProjectId.eq(project_id))
            .all(&txn)
            .await?;
        if labels_in_project.len() != payload.label_ids.len() {
            return Err(AppError::BadRequest);
        }
    }
    for lid in &payload.label_ids {
        task_labels::ActiveModel {
            task_id: Set(model.id),
            label_id: Set(*lid),
        }
        .insert(&txn)
        .await?;
    }

    txn.commit().await?;
    Ok((StatusCode::CREATED, Json(model)))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Tasks",
    summary = "タスク取得",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID（UUID または ENG-42 形式）"),
    ),
    responses(
        (status = 200, description = "タスク", body = tasks::Model),
        CrudErrors,
    )
)]
pub async fn get_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<tasks::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    Ok(Json(task))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Tasks",
    summary = "タスク更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    request_body = UpdateTaskRequest,
    responses(
        (status = 200, description = "更新後のタスク", body = tasks::Model),
        CrudErrors,
    )
)]
pub async fn update_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
    Valid(Json(payload)): Valid<Json<UpdateTaskRequest>>,
) -> Result<Json<tasks::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    // 親タスク変更時の循環参照検出（修正3）
    // clear_parent_task_id が優先されるため、クリア指定時はチェック不要。
    if !payload.clear_parent_task_id {
        if let Some(new_parent_id) = payload.parent_task_id {
            // 自己参照（自分自身を親に設定）を拒否
            if new_parent_id == task.id {
                return Err(AppError::Conflict);
            }
            // task が new_parent_id の祖先なら、親に設定するとサイクルが生じる
            if is_ancestor_of(&state.db, project_id, task.id, new_parent_id).await? {
                return Err(AppError::Conflict);
            }
        }
    }

    let mut active: tasks::ActiveModel = task.into();
    if let Some(v) = payload.title { active.title = Set(v); }
    if payload.clear_description {
        active.description = Set(None);
    } else if let Some(v) = payload.description {
        active.description = Set(Some(v));
    }
    if let Some(v) = payload.status_id {
        project_statuses::Entity::find_by_id(v)
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
        active.status_id = Set(v);
    }
    if let Some(v) = payload.priority { active.priority = Set(v); }
    if let Some(v) = payload.progress_pct { active.progress_pct = Set(v); }
    if payload.clear_parent_task_id {
        active.parent_task_id = Set(None);
    } else if let Some(v) = payload.parent_task_id {
        tasks::Entity::find_by_id(v)
            .filter(tasks::Column::ProjectId.eq(project_id))
            .filter(tasks::Column::DeletedAt.is_null())
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
        active.parent_task_id = Set(Some(v));
    }
    if payload.clear_milestone_id {
        active.milestone_id = Set(None);
    } else if let Some(v) = payload.milestone_id {
        milestones::Entity::find_by_id(v)
            .filter(milestones::Column::ProjectId.eq(project_id))
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
        active.milestone_id = Set(Some(v));
    }
    if payload.clear_soft_deadline { active.soft_deadline = Set(None); }
    else if let Some(v) = payload.soft_deadline { active.soft_deadline = Set(Some(v)); }
    if payload.clear_hard_deadline { active.hard_deadline = Set(None); }
    else if let Some(v) = payload.hard_deadline { active.hard_deadline = Set(Some(v)); }
    if payload.clear_estimated_minutes { active.estimated_minutes = Set(None); }
    else if let Some(v) = payload.estimated_minutes { active.estimated_minutes = Set(Some(v)); }
    if let Some(v) = payload.is_archived { active.is_archived = Set(v); }
    active.updated_at = Set(chrono::Utc::now());

    Ok(Json(active.update(&state.db).await?))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Tasks",
    summary = "タスク削除（ソフト）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    if task.created_by != auth.user_id && !is_tenant_owner(&state, tenant_id, auth.user_id).await? {
        return Err(AppError::Forbidden);
    }
    let mut active: tasks::ActiveModel = task.into();
    active.deleted_at = Set(Some(chrono::Utc::now()));
    active.update(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/archive",
    tag = "Tasks",
    summary = "タスクをアーカイブ",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    responses(
        (status = 200, description = "アーカイブ後のタスク", body = tasks::Model),
        CrudErrors,
    )
)]
pub async fn archive_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<tasks::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    let mut active: tasks::ActiveModel = task.into();
    active.is_archived = Set(true);
    active.updated_at = Set(chrono::Utc::now());
    Ok(Json(active.update(&state.db).await?))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/unarchive",
    tag = "Tasks",
    summary = "タスクのアーカイブ解除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    responses(
        (status = 200, description = "アーカイブ解除後のタスク", body = tasks::Model),
        CrudErrors,
    )
)]
pub async fn unarchive_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<tasks::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    let mut active: tasks::ActiveModel = task.into();
    active.is_archived = Set(false);
    active.updated_at = Set(chrono::Utc::now());
    Ok(Json(active.update(&state.db).await?))
}

// ─── Assignees ───────────────────────────────────────────────────────────

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}/assignees",
    tag = "Tasks",
    summary = "担当者一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    responses(
        (status = 200, description = "担当者一覧", body = [task_assignees::Model]),
        CrudErrors,
    )
)]
pub async fn list_assignees(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<Vec<task_assignees::Model>>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    let assignees = task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.eq(task.id))
        .all(&state.db)
        .await?;
    Ok(Json(assignees))
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct AddAssigneeRequest {
    pub user_id: Uuid,
    #[validate(length(min = 1))]
    pub role: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/assignees",
    tag = "Tasks",
    summary = "担当者追加",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    request_body = AddAssigneeRequest,
    responses(
        (status = 201, description = "追加された担当者", body = task_assignees::Model),
        CrudErrors,
    )
)]
pub async fn add_assignee(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
    Valid(Json(payload)): Valid<Json<AddAssigneeRequest>>,
) -> Result<(StatusCode, Json<task_assignees::Model>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    require_member_or_owner(&state, tenant_id, project_id, payload.user_id).await?;
    let assignee = task_assignees::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(task.id),
        user_id: Set(payload.user_id),
        role: Set(payload.role),
        assigned_at: Set(chrono::Utc::now()),
    }
    .insert(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(assignee)))
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateAssigneeRequest {
    #[validate(length(min = 1))]
    pub role: String,
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}/assignees/{user_id}",
    tag = "Tasks",
    summary = "担当者ロール変更",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    request_body = UpdateAssigneeRequest,
    responses(
        (status = 200, description = "更新後の担当者", body = task_assignees::Model),
        CrudErrors,
    )
)]
pub async fn update_assignee(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, user_id)): Path<(Uuid, Uuid, String, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateAssigneeRequest>>,
) -> Result<Json<task_assignees::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    let assignee = task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.eq(task.id))
        .filter(task_assignees::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut active: task_assignees::ActiveModel = assignee.into();
    active.role = Set(payload.role);
    Ok(Json(active.update(&state.db).await?))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/assignees/{user_id}",
    tag = "Tasks",
    summary = "担当者削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn remove_assignee(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, user_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    let assignee = task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.eq(task.id))
        .filter(task_assignees::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    task_assignees::Entity::delete_by_id(assignee.id).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Relations ───────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct RelationEntry {
    pub relation_id: Uuid,
    #[serde(flatten)]
    pub task: tasks::Model,
}

#[derive(Serialize, ToSchema)]
pub struct TaskRelationsResponse {
    pub subtasks: Vec<tasks::Model>,
    pub blocks: Vec<RelationEntry>,
    pub blocked_by: Vec<RelationEntry>,
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}/relations",
    tag = "Tasks",
    summary = "依存関係一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    responses(
        (status = 200, description = "依存関係", body = TaskRelationsResponse),
        CrudErrors,
    )
)]
pub async fn list_relations(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<TaskRelationsResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let subtasks = tasks::Entity::find()
        .filter(tasks::Column::ParentTaskId.eq(task.id))
        .filter(tasks::Column::DeletedAt.is_null())
        .all(&state.db)
        .await?;

    let blocks_rels = task_relations::Entity::find()
        .filter(task_relations::Column::BlockerTaskId.eq(task.id))
        .all(&state.db)
        .await?;
    let mut blocks = Vec::new();
    for rel in blocks_rels {
        if let Some(t) = tasks::Entity::find_by_id(rel.blocked_task_id)
            .filter(tasks::Column::ProjectId.eq(project_id))
            .filter(tasks::Column::DeletedAt.is_null())
            .one(&state.db)
            .await?
        {
            blocks.push(RelationEntry { relation_id: rel.id, task: t });
        }
    }

    let blocked_rels = task_relations::Entity::find()
        .filter(task_relations::Column::BlockedTaskId.eq(task.id))
        .all(&state.db)
        .await?;
    let mut blocked_by = Vec::new();
    for rel in blocked_rels {
        if let Some(t) = tasks::Entity::find_by_id(rel.blocker_task_id)
            .filter(tasks::Column::ProjectId.eq(project_id))
            .filter(tasks::Column::DeletedAt.is_null())
            .one(&state.db)
            .await?
        {
            blocked_by.push(RelationEntry { relation_id: rel.id, task: t });
        }
    }

    Ok(Json(TaskRelationsResponse { subtasks, blocks, blocked_by }))
}

#[derive(Deserialize, ToSchema)]
pub struct AddRelationRequest {
    #[serde(rename = "type")]
    pub relation_type: String,
    pub target_task_id: Uuid,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/relations",
    tag = "Tasks",
    summary = "依存関係追加（循環検出あり）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    request_body = AddRelationRequest,
    responses(
        (status = 201, description = "追加された依存関係", body = task_relations::Model),
        CrudErrors,
    )
)]
pub async fn add_relation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
    Json(payload): Json<AddRelationRequest>,
) -> Result<(StatusCode, Json<task_relations::Model>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    resolve_task(
        &state,
        tenant_id,
        project_id,
        &payload.target_task_id.to_string(),
    )
    .await?;

    let (blocker, blocked) = match payload.relation_type.as_str() {
        "blocks" => (task.id, payload.target_task_id),
        "blocked_by" => (payload.target_task_id, task.id),
        _ => return Err(AppError::BadRequest),
    };

    // SERIALIZABLE: prevent concurrent inverse relations from bypassing cycle detection
    // (READ COMMITTED allows both T1/T2 to pass would_create_cycle before either commits).
    let txn = state
        .db
        .begin_with_config(Some(IsolationLevel::Serializable), None)
        .await?;
    if would_create_cycle(&txn, project_id, blocker, blocked).await? {
        return Err(AppError::Conflict);
    }

    let rel = task_relations::ActiveModel {
        id: Set(Uuid::new_v4()),
        blocker_task_id: Set(blocker),
        blocked_task_id: Set(blocked),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;

    Ok((StatusCode::CREATED, Json(rel)))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/relations/{relation_id}",
    tag = "Tasks",
    summary = "依存関係削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("relation_id" = Uuid, Path, description = "依存関係ID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn remove_relation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, relation_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    let rel = task_relations::Entity::find_by_id(relation_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if rel.blocker_task_id != task.id && rel.blocked_task_id != task.id {
        return Err(AppError::NotFound);
    }
    task_relations::Entity::delete_by_id(relation_id).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

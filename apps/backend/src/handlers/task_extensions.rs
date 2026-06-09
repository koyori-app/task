//! 全文検索・バルク操作・保存済みビュー・ファイル添付。

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Statement, TransactionTrait,
    prelude::Uuid,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth_helpers::{is_tenant_owner, require_member_or_owner};
use crate::entities::{
    drive_files, labels, project_statuses, project_task_views, sprints, task_assignees,
    task_attachments, task_labels, tasks,
};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use crate::utils::drive::content_url;
use crate::utils::task_activities::{record_activity, status_name};
use crate::AppState;

const BULK_MAX_TASKS: usize = 100;

fn use_pg_bigm_search() -> bool {
    std::env::var("USE_PG_BIGM")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub struct SearchTasksQuery {
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_search_limit() -> u64 {
    20
}

#[derive(Serialize, ToSchema)]
pub struct SearchTaskHit {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub seq_id: i32,
    pub title: String,
    pub highlight: String,
    pub score: f32,
}

#[derive(Serialize, ToSchema)]
pub struct SearchTasksResponse {
    pub tasks: Vec<SearchTaskHit>,
    pub total: u64,
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/search",
    tag = "Tasks",
    summary = "タスク全文検索",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        SearchTasksQuery,
    ),
    responses(
        (status = 200, description = "検索結果", body = SearchTasksResponse),
        CrudErrors,
    )
)]
pub async fn search_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<SearchTasksQuery>,
) -> Result<Json<SearchTasksResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let query = q.q.trim();
    if query.is_empty() {
        return Err(AppError::BadRequest);
    }

    let limit = q.limit.min(100);
    let offset = q.offset;

    if use_pg_bigm_search() {
        search_tasks_tsvector(&state, project_id, query, limit, offset).await
    } else {
        search_tasks_ilike(&state, project_id, query, limit, offset).await
    }
}

async fn search_tasks_ilike(
    state: &AppState,
    project_id: Uuid,
    query: &str,
    limit: u64,
    offset: u64,
) -> Result<Json<SearchTasksResponse>, AppError> {
    let pattern = format!("%{query}%");
    let base = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(Expr::cust_with_values(
            "(COALESCE(title, '') ILIKE $1 OR COALESCE(description, '') ILIKE $1)",
            [pattern.clone()],
        ));

    let total = base.clone().count(&state.db).await?;
    let rows = base
        .order_by_desc(tasks::Column::UpdatedAt)
        .limit(limit)
        .offset(offset)
        .all(&state.db)
        .await?;

    let hits = rows
        .into_iter()
        .map(|t| SearchTaskHit {
            id: t.id,
            seq_id: t.seq_id,
            title: t.title.clone(),
            highlight: highlight_ilike(&t.title, t.description.as_deref(), query),
            score: 1.0,
        })
        .collect();

    Ok(Json(SearchTasksResponse { tasks: hits, total }))
}

fn highlight_ilike(title: &str, description: Option<&str>, query: &str) -> String {
    let q_lower = query.to_lowercase();
    let text = format!("{} {}", title, description.unwrap_or_default());
    if let Some(pos) = text.to_lowercase().find(&q_lower) {
        let end = (pos + query.len()).min(text.len());
        format!(
            "…{}<em>{}</em>{}…",
            &text[..pos],
            &text[pos..end],
            &text[end..]
        )
    } else {
        text.chars().take(120).collect()
    }
}

async fn search_tasks_tsvector(
    state: &AppState,
    project_id: Uuid,
    query: &str,
    limit: u64,
    offset: u64,
) -> Result<Json<SearchTasksResponse>, AppError> {
    let count_sql = r#"
        SELECT COUNT(*)::bigint AS cnt
        FROM tasks
        WHERE project_id = $1
          AND deleted_at IS NULL
          AND search_vector @@ plainto_tsquery('pg_catalog.simple', $2)
    "#;
    let count_result = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            count_sql,
            [project_id.into(), query.into()],
        ))
        .await?;
    let total: i64 = count_result
        .and_then(|r| r.try_get_by_index(0).ok())
        .unwrap_or(0);

    let search_sql = r#"
        SELECT id, seq_id, title,
               ts_rank(search_vector, plainto_tsquery('pg_catalog.simple', $2))::real AS score,
               ts_headline(
                   'pg_catalog.simple',
                   coalesce(title, '') || ' ' || coalesce(description, ''),
                   plainto_tsquery('pg_catalog.simple', $2)
               ) AS highlight
        FROM tasks
        WHERE project_id = $1
          AND deleted_at IS NULL
          AND search_vector @@ plainto_tsquery('pg_catalog.simple', $2)
        ORDER BY score DESC
        LIMIT $3 OFFSET $4
    "#;
    let rows = state
        .db
        .query_all_raw(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            search_sql,
            [
                project_id.into(),
                query.into(),
                (limit as i64).into(),
                (offset as i64).into(),
            ],
        ))
        .await?;
    let hits = rows
        .into_iter()
        .filter_map(|row| {
            Some(SearchTaskHit {
                id: row.try_get_by_index(0).ok()?,
                seq_id: row.try_get_by_index(1).ok()?,
                title: row.try_get_by_index(2).ok()?,
                score: row.try_get_by_index(3).ok()?,
                highlight: row.try_get_by_index(4).ok()?,
            })
        })
        .collect();

    Ok(Json(SearchTasksResponse {
        tasks: hits,
        total: total as u64,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct BulkUpdateFields {
    #[schema(value_type = Option<String>, format = "uuid")]
    pub status_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub assignee_id: Option<Uuid>,
    pub label_ids: Option<Vec<Uuid>>,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub sprint_id: Option<Uuid>,
    #[serde(default)]
    pub clear_sprint_id: bool,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct BulkUpdateRequest {
    #[validate(length(min = 1))]
    pub task_ids: Vec<Uuid>,
    pub update: BulkUpdateFields,
}

#[derive(Serialize, ToSchema)]
pub struct BulkUpdateResponse {
    pub updated: u32,
    pub failed: Vec<BulkFailure>,
}

#[derive(Serialize, ToSchema)]
pub struct BulkFailure {
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    pub reason: String,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/bulk",
    tag = "Tasks",
    summary = "タスク一括更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = BulkUpdateRequest,
    responses(
        (status = 200, description = "一括更新結果", body = BulkUpdateResponse),
        CrudErrors,
    )
)]
pub async fn bulk_update_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<BulkUpdateRequest>>,
) -> Result<Json<BulkUpdateResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    if payload.task_ids.len() > BULK_MAX_TASKS {
        return Err(AppError::BadRequest);
    }

    let mut unique_ids = payload.task_ids.clone();
    unique_ids.sort();
    unique_ids.dedup();

    let mut updated = 0u32;
    let mut failed = Vec::new();

    for task_id in unique_ids {
        match apply_bulk_update(
            &state,
            tenant_id,
            project_id,
            task_id,
            auth.user_id,
            &payload.update,
        )
        .await
        {
            Ok(()) => updated += 1,
            Err(e) => failed.push(BulkFailure {
                task_id,
                reason: bulk_error_reason(&e),
            }),
        }
    }

    Ok(Json(BulkUpdateResponse { updated, failed }))
}

fn bulk_error_reason(err: &AppError) -> String {
    match err {
        AppError::NotFound => "not-found".into(),
        AppError::Forbidden => "forbidden".into(),
        AppError::Conflict => "conflict".into(),
        AppError::BadRequest => "bad-request".into(),
        AppError::BadRequestDetail(d) => d.clone(),
        _ => "error".into(),
    }
}

async fn apply_bulk_update(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    user_id: Uuid,
    update: &BulkUpdateFields,
) -> Result<(), AppError> {
    let task = tasks::Entity::find_by_id(task_id)
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let txn = state.db.begin().await?;
    let snapshot = task.clone();
    let mut active: tasks::ActiveModel = task.into();

    if let Some(status_id) = update.status_id {
        let status = project_statuses::Entity::find_by_id(status_id)
            .filter(project_statuses::Column::ProjectId.eq(project_id))
            .one(&txn)
            .await?
            .ok_or(AppError::NotFound)?;
        active.status_id = Set(status_id);
        active.completed_at = if status.is_done_state {
            Set(Some(chrono::Utc::now()))
        } else {
            Set(None)
        };
        let from = status_name(&txn, snapshot.status_id).await?;
        let to = status_name(&txn, status_id).await?;
        record_activity(
            &txn,
            task_id,
            Some(user_id),
            "status_changed",
            serde_json::json!({ "from": from, "to": to }).into(),
        )
        .await?;
    }

    if update.clear_sprint_id {
        active.sprint_id = Set(None);
    } else if let Some(sprint_id) = update.sprint_id {
        sprints::Entity::find_by_id(sprint_id)
            .filter(sprints::Column::ProjectId.eq(project_id))
            .one(&txn)
            .await?
            .ok_or(AppError::NotFound)?;
        active.sprint_id = Set(Some(sprint_id));
    }

    active.updated_at = Set(chrono::Utc::now());
    active.update(&txn).await?;

    if let Some(assignee_id) = update.assignee_id {
        require_member_or_owner(state, tenant_id, project_id, assignee_id).await?;
        let exists = task_assignees::Entity::find()
            .filter(task_assignees::Column::TaskId.eq(task_id))
            .filter(task_assignees::Column::UserId.eq(assignee_id))
            .one(&txn)
            .await?
            .is_some();
        if !exists {
            task_assignees::ActiveModel {
                id: Set(Uuid::new_v4()),
                task_id: Set(task_id),
                user_id: Set(assignee_id),
                role: Set("assignee".into()),
                assigned_at: Set(chrono::Utc::now()),
            }
            .insert(&txn)
            .await?;
            record_activity(
                &txn,
                task_id,
                Some(user_id),
                "assignee_added",
                serde_json::json!({ "user_id": assignee_id }).into(),
            )
            .await?;
        }
    }

    if let Some(ref label_ids) = update.label_ids {
        let mut unique = label_ids.clone();
        unique.sort();
        unique.dedup();
        if !unique.is_empty() {
            let in_project = labels::Entity::find()
                .filter(labels::Column::Id.is_in(unique.clone()))
                .filter(labels::Column::ProjectId.eq(project_id))
                .all(&txn)
                .await?;
            if in_project.len() != unique.len() {
                return Err(AppError::BadRequest);
            }
            for lid in unique {
                let exists = task_labels::Entity::find()
                    .filter(task_labels::Column::TaskId.eq(task_id))
                    .filter(task_labels::Column::LabelId.eq(lid))
                    .one(&txn)
                    .await?
                    .is_some();
                if !exists {
                    task_labels::ActiveModel {
                        task_id: Set(task_id),
                        label_id: Set(lid),
                    }
                    .insert(&txn)
                    .await?;
                }
            }
        }
    }

    txn.commit().await?;
    Ok(())
}

#[derive(Serialize, ToSchema)]
pub struct TaskViewListResponse {
    pub views: Vec<project_task_views::Model>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateTaskViewRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[serde(default)]
    pub is_shared: bool,
    #[serde(default)]
    pub filters: serde_json::Value,
    #[serde(default)]
    pub sort: serde_json::Value,
    #[serde(default = "default_view_type")]
    pub view_type: String,
}

fn default_view_type() -> String {
    "list".into()
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateTaskViewRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub is_shared: Option<bool>,
    pub filters: Option<serde_json::Value>,
    pub sort: Option<serde_json::Value>,
    pub view_type: Option<String>,
}

#[axum::debug_handler]
#[utoipa::path(get, path = "/", tag = "TaskViews", summary = "保存済みビュー一覧")]
pub async fn list_task_views(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TaskViewListResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let views = project_task_views::Entity::find()
        .filter(project_task_views::Column::ProjectId.eq(project_id))
        .filter(
            Condition::any()
                .add(project_task_views::Column::CreatedBy.eq(auth.user_id))
                .add(project_task_views::Column::IsShared.eq(true)),
        )
        .order_by_asc(project_task_views::Column::Name)
        .all(&state.db)
        .await?;

    Ok(Json(TaskViewListResponse { views }))
}

#[axum::debug_handler]
#[utoipa::path(post, path = "/", tag = "TaskViews", summary = "保存済みビュー作成")]
pub async fn create_task_view(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateTaskViewRequest>>,
) -> Result<(StatusCode, Json<project_task_views::Model>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let model = project_task_views::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        created_by: Set(auth.user_id),
        name: Set(payload.name),
        is_shared: Set(payload.is_shared),
        filters: Set(payload.filters.into()),
        sort: Set(payload.sort.into()),
        view_type: Set(payload.view_type),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(model)))
}

#[axum::debug_handler]
#[utoipa::path(patch, path = "/{view_id}", tag = "TaskViews", summary = "保存済みビュー更新")]
pub async fn update_task_view(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, view_id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateTaskViewRequest>>,
) -> Result<Json<project_task_views::Model>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let view = project_task_views::Entity::find_by_id(view_id)
        .filter(project_task_views::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let owner = is_tenant_owner(&state, tenant_id, auth.user_id).await?;
    if view.created_by != auth.user_id && !owner {
        return Err(AppError::Forbidden);
    }

    let mut active: project_task_views::ActiveModel = view.into();
    if let Some(name) = payload.name {
        active.name = Set(name);
    }
    if let Some(is_shared) = payload.is_shared {
        active.is_shared = Set(is_shared);
    }
    if let Some(filters) = payload.filters {
        active.filters = Set(filters.into());
    }
    if let Some(sort) = payload.sort {
        active.sort = Set(sort.into());
    }
    if let Some(view_type) = payload.view_type {
        active.view_type = Set(view_type);
    }

    let updated = active.update(&state.db).await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(delete, path = "/{view_id}", tag = "TaskViews", summary = "保存済みビュー削除")]
pub async fn delete_task_view(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, view_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;

    let view = project_task_views::Entity::find_by_id(view_id)
        .filter(project_task_views::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let owner = is_tenant_owner(&state, tenant_id, auth.user_id).await?;
    if view.created_by != auth.user_id && !owner {
        return Err(AppError::Forbidden);
    }

    project_task_views::Entity::delete_by_id(view_id)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct TaskAttachmentResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub drive_file_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size: i64,
    pub url: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct TaskAttachmentListResponse {
    pub attachments: Vec<TaskAttachmentResponse>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct AttachFileRequest {
    #[schema(value_type = String, format = "uuid")]
    pub drive_file_id: Uuid,
}

#[axum::debug_handler]
#[utoipa::path(get, path = "/{id}/attachments", tag = "Tasks", summary = "タスク添付ファイル一覧")]
pub async fn list_task_attachments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<TaskAttachmentListResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let rows = task_attachments::Entity::find()
        .filter(task_attachments::Column::TaskId.eq(task.id))
        .order_by_desc(task_attachments::Column::CreatedAt)
        .all(&state.db)
        .await?;

    let mut attachments = Vec::with_capacity(rows.len());
    for row in rows {
        let file = drive_files::Entity::find_by_id(row.drive_file_id)
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
        attachments.push(TaskAttachmentResponse {
            id: row.id,
            drive_file_id: row.drive_file_id,
            name: file.name,
            mime_type: file.mime_type,
            size: file.size,
            url: content_url(file.id),
            created_at: row.created_at,
        });
    }

    Ok(Json(TaskAttachmentListResponse { attachments }))
}

#[axum::debug_handler]
#[utoipa::path(post, path = "/{id}/attachments", tag = "Tasks", summary = "タスクにファイルを添付")]
pub async fn attach_task_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
    Valid(Json(payload)): Valid<Json<AttachFileRequest>>,
) -> Result<(StatusCode, Json<TaskAttachmentResponse>), AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let file = drive_files::Entity::find_by_id(payload.drive_file_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if file.tenant_id != tenant_id {
        return Err(AppError::Forbidden);
    }
    if let Some(pid) = file.project_id {
        if pid != project_id {
            return Err(AppError::Forbidden);
        }
    }

    let model = task_attachments::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(task.id),
        drive_file_id: Set(payload.drive_file_id),
        created_by: Set(auth.user_id),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(TaskAttachmentResponse {
            id: model.id,
            drive_file_id: file.id,
            name: file.name,
            mime_type: file.mime_type,
            size: file.size,
            url: content_url(file.id),
            created_at: model.created_at,
        }),
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/attachments/{attachment_id}",
    tag = "Tasks",
    summary = "タスク添付を解除"
)]
pub async fn detach_task_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, attachment_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let result = task_attachments::Entity::delete_many()
        .filter(task_attachments::Column::Id.eq(attachment_id))
        .filter(task_attachments::Column::TaskId.eq(task.id))
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

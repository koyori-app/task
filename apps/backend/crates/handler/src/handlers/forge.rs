//! タスクに結ばれた PR / コミット / ブランチ（docs/features/tasks/9.github-tasks.md §8）。

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, EntityTrait, FromQueryResult, QueryFilter, QueryOrder, Statement, prelude::Uuid,
};

use crate::AppState;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use entity::forge_branch_links;
use payload::forge::{TaskForgeBranch, TaskForgeCommit, TaskForgePullRequest, TaskForgeResponse};

#[derive(FromQueryResult)]
struct PullRequestRow {
    id: Uuid,
    host: String,
    host_url: String,
    repo_owner: String,
    repo_name: String,
    number: i32,
    title: String,
    author_handle: String,
    state: String,
    html_url: String,
    merged_at: Option<DateTime<Utc>>,
    merge_effects_processed_at: Option<DateTime<Utc>>,
    commits_complete: bool,
    /// 参照元をカンマで連結したもの（昇順）
    sources: String,
    closes: bool,
}

#[derive(FromQueryResult)]
struct CommitRow {
    id: Uuid,
    host: String,
    host_url: String,
    repo_owner: String,
    repo_name: String,
    sha: String,
    message: String,
    author_handle: String,
    author_name: String,
    committed_at: DateTime<Utc>,
    html_url: String,
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}/forge",
    tag = "Tasks",
    summary = "タスクに結ばれた PR・コミット・ブランチ",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    responses(
        (status = 200, description = "PR・コミット・ブランチ", body = TaskForgeResponse),
        CrudErrors,
    )
)]
pub async fn get_task_forge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<TaskForgeResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    // PR 行のプロジェクトはタスクのプロジェクトと限らない（KEY-N はテナント内で解決する）が、
    // 返すのはこのタスクに結ばれたものだけなので、タスクの閲覧権限で足りる
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let backend = state.db.get_database_backend();
    let pull_requests = PullRequestRow::find_by_statement(Statement::from_sql_and_values(
        backend,
        "SELECT p.id, p.host, p.host_url, p.repo_owner, p.repo_name, p.number, p.title,
                p.author_handle, p.state, p.html_url, p.merged_at, p.merge_effects_processed_at,
                p.commits_complete,
                string_agg(l.source, ',' ORDER BY l.source) AS sources,
                bool_or(l.closes) AS closes
         FROM forge_pull_requests p
         JOIN forge_pull_request_links l ON l.pull_request_id = p.id
         WHERE l.task_id = $1
         GROUP BY p.id
         ORDER BY p.created_at DESC, p.id DESC",
        [task.id.into()],
    ))
    .all(&state.db)
    .await?
    .into_iter()
    .map(|row| TaskForgePullRequest {
        id: row.id,
        host: row.host,
        host_url: row.host_url,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        number: row.number,
        title: row.title,
        author_handle: row.author_handle,
        state: row.state,
        html_url: row.html_url,
        merged_at: row.merged_at,
        merge_effects_processed_at: row.merge_effects_processed_at,
        commits_complete: row.commits_complete,
        sources: row.sources.split(',').map(str::to_owned).collect(),
        closes: row.closes,
        unresolved: None,
    })
    .collect();

    let commits = CommitRow::find_by_statement(Statement::from_sql_and_values(
        backend,
        "SELECT c.id, c.host, c.host_url, c.repo_owner, c.repo_name, c.sha, c.message,
                c.author_handle, c.author_name, c.committed_at, c.html_url
         FROM forge_commits c
         JOIN forge_commit_links l ON l.commit_id = c.id
         WHERE l.task_id = $1
         ORDER BY c.committed_at DESC, c.id DESC",
        [task.id.into()],
    ))
    .all(&state.db)
    .await?
    .into_iter()
    .map(|row| TaskForgeCommit {
        id: row.id,
        host: row.host,
        host_url: row.host_url,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        sha: row.sha,
        message: row.message.lines().next().unwrap_or_default().to_owned(),
        author_handle: row.author_handle,
        author_name: row.author_name,
        committed_at: row.committed_at,
        html_url: row.html_url,
    })
    .collect();

    Ok(Json(TaskForgeResponse {
        pull_requests,
        commits,
        branches: task_branches(&state, task.id).await?,
    }))
}

async fn task_branches(state: &AppState, task_id: Uuid) -> Result<Vec<TaskForgeBranch>, AppError> {
    Ok(forge_branch_links::Entity::find()
        .filter(forge_branch_links::Column::TaskId.eq(task_id))
        .order_by_desc(forge_branch_links::Column::CreatedAt)
        .order_by_desc(forge_branch_links::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(|row| TaskForgeBranch {
            id: row.id,
            host: row.host,
            host_url: row.host_url,
            repo_owner: row.repo_owner,
            repo_name: row.repo_name,
            branch: row.branch,
            html_url: row.html_url,
            created_at: row.created_at.into(),
        })
        .collect())
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/forge/branches/{link_id}",
    tag = "Tasks",
    summary = "ブランチのリンクを外す",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("link_id" = Uuid, Path, description = "ブランチのリンクID"),
    ),
    responses(
        (status = 204, description = "外しました"),
        CrudErrors,
    )
)]
pub async fn delete_task_forge_branch(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, link_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    // 他タスクの link_id は存在を明かさず 404
    let deleted = forge_branch_links::Entity::delete_many()
        .filter(forge_branch_links::Column::Id.eq(link_id))
        .filter(forge_branch_links::Column::TaskId.eq(task.id))
        .exec(&state.db)
        .await?;
    if deleted.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

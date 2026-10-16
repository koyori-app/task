//! タスクに結ばれた PR / コミット / ブランチ（docs/features/tasks/9.github-tasks.md §8）。

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, FromQueryResult, QueryFilter, QueryOrder,
    Statement, prelude::Uuid,
};

use crate::AppState;
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use entity::{forge_branch_links, github_integrations};
use payload::forge::{
    LinkTaskPullRequestRequest, TaskForgeBranch, TaskForgeCommit, TaskForgePullRequest,
    TaskForgeRepository, TaskForgeResponse,
};
use service::forge::events::ForgeRepo;

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

    let pull_requests = task_pull_requests(&state.db, task.id, None).await?;

    let backend = state.db.get_database_backend();
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

    let repository = project_repository(&state.db, project_id)
        .await?
        .map(|repo| TaskForgeRepository {
            html_url: format!("{}/{}/{}", repo.host_url, repo.repo_owner, repo.repo_name),
            host: repo.host,
            host_url: repo.host_url,
            repo_owner: repo.repo_owner,
            repo_name: repo.repo_name,
        });

    Ok(Json(TaskForgeResponse {
        pull_requests,
        commits,
        branches: task_branches(&state, task.id).await?,
        repository,
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

/// プロジェクトの現在の連携先（GitHub だけ）
async fn project_repository(
    db: &DatabaseConnection,
    project_id: Uuid,
) -> Result<Option<ForgeRepo>, AppError> {
    Ok(github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(project_id))
        .one(db)
        .await?
        .map(|integration| ForgeRepo {
            host: "github".into(),
            host_url: "https://github.com".into(),
            repo_owner: integration.repo_owner,
            repo_name: integration.repo_name,
        }))
}

/// タスクに結ばれた PR を参照元ごとの行からまとめる。`only` を渡すとその PR だけ
async fn task_pull_requests(
    db: &DatabaseConnection,
    task_id: Uuid,
    only: Option<Uuid>,
) -> Result<Vec<TaskForgePullRequest>, AppError> {
    Ok(
        PullRequestRow::find_by_statement(Statement::from_sql_and_values(
            db.get_database_backend(),
            "SELECT p.id, p.host, p.host_url, p.repo_owner, p.repo_name, p.number, p.title,
                p.author_handle, p.state, p.html_url, p.merged_at, p.merge_effects_processed_at,
                p.commits_complete,
                string_agg(l.source, ',' ORDER BY l.source) AS sources,
                bool_or(l.closes) AS closes
         FROM forge_pull_requests p
         JOIN forge_pull_request_links l ON l.pull_request_id = p.id
         WHERE l.task_id = $1 AND ($2::uuid IS NULL OR p.id = $2)
         GROUP BY p.id
         ORDER BY p.created_at DESC, p.id DESC",
            [task_id.into(), only.into()],
        ))
        .all(db)
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
        .collect(),
    )
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/forge/pull-requests",
    tag = "Tasks",
    summary = "PR を手動でタスクにリンク",
    description = "リポジトリはプロジェクトの現在の GitHub 連携先で決まる。連携が無ければ 409。\
                   ホストから PR を取れなくても `state = unknown` で作ってリンクする",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
    ),
    request_body = LinkTaskPullRequestRequest,
    responses(
        (status = 201, description = "リンクした PR", body = TaskForgePullRequest),
        (status = 409, description = "プロジェクトが GitHub と連携していない（github-integration-missing）", body = ServerError),
        CrudErrors,
    )
)]
pub async fn link_task_pull_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
    Valid(Json(body)): Valid<Json<LinkTaskPullRequestRequest>>,
) -> Result<(StatusCode, Json<TaskForgePullRequest>), AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let repo = project_repository(&state.db, project_id)
        .await?
        .ok_or_else(|| AppError::ConflictDetail("github-integration-missing".into()))?;

    // 取れなくても（App 未設定・404・通信断）リンクは作る。中身は次の webhook で埋まる
    let fetched = match state.settings.require_github_app() {
        Ok(github) => {
            service::github::client::fetch_pull_request(
                &state.db,
                &state.http_client,
                github,
                project_id,
                &repo,
                body.number,
            )
            .await
        }
        Err(e) => Err(anyhow::anyhow!("github app settings are required: {e}")),
    };
    match fetched {
        // webhook と同じ共通処理を通し、PR 行と title / body / branch 由来のリンクを同じ世代に揃える
        Ok(snapshot) => {
            service::forge::pull_requests::apply_pull_request(
                &state.db,
                project_id,
                &repo,
                body.number,
                snapshot.clone(),
                || async { Ok(snapshot) },
            )
            .await?;
        }
        Err(e) => tracing::warn!(
            error = %e,
            %project_id,
            number = body.number,
            "fetch pull request for manual link failed"
        ),
    }

    let html_url = format!(
        "{}/{}/{}/pull/{}",
        repo.host_url, repo.repo_owner, repo.repo_name, body.number
    );
    let pull_request_id = service::forge::pull_requests::link_manually(
        &state.db,
        project_id,
        &repo,
        body.number,
        &html_url,
        task.id,
        body.closes,
        auth.user_id,
    )
    .await?;
    let pull_request = task_pull_requests(&state.db, task.id, Some(pull_request_id))
        .await?
        .pop()
        .ok_or_else(|| anyhow::anyhow!("linked pull request {pull_request_id} is missing"))?;
    Ok((StatusCode::CREATED, Json(pull_request)))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/forge/pull-requests/{pull_request_id}",
    tag = "Tasks",
    summary = "PR のリンクを外す",
    description = "その PR とタスクのリンクを参照元によらずすべて外す。PR 自体は残る。\
                   タイトル・本文に KEY-N が残っていれば、次の PR イベントで自動リンクが戻る",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("pull_request_id" = Uuid, Path, description = "PR の ID"),
    ),
    responses(
        (status = 204, description = "外した"),
        CrudErrors,
    )
)]
pub async fn unlink_task_pull_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, pull_request_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    if !service::forge::pull_requests::unlink(&state.db, task.id, pull_request_id, auth.user_id)
        .await?
    {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
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

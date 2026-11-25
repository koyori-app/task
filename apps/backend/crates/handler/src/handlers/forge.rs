//! タスクに結ばれた PR / コミット / ブランチ（docs/dev/features/tasks/github-tasks/index.md の「API」）。

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
use std::collections::HashMap;

use crate::AppState;
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use entity::{forge_branch_links, github_integrations, projects, tasks};
use payload::forge::{
    LinkTaskPullRequestRequest, SetTaskPullRequestRoleRequest, TaskForgeBranch, TaskForgeCommit,
    TaskForgePullRequest, TaskForgePullRequestRef, TaskForgeRepository, TaskForgeResponse,
};
use service::forge::events::ForgeRepo;
use service::forge::pull_requests::{PullRequestRole, pull_request_role};

#[derive(FromQueryResult)]
struct PullRequestRow {
    id: Uuid,
    project_id: Uuid,
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
    head_branch: Option<String>,
    /// 手の上書き（`forge_pull_request_roles`）。無ければ NULL
    role_override: Option<String>,
}

/// コミットやブランチに付ける PR。`owner` はそれを付ける相手（コミットの id / ブランチのリンクの id）
#[derive(FromQueryResult)]
struct PullRequestRefRow {
    owner: Uuid,
    number: i32,
    state: String,
    html_url: String,
}

/// `owner` ごとにまとめる。行は番号の昇順で渡す
fn group_pull_request_refs(
    rows: Vec<PullRequestRefRow>,
) -> HashMap<Uuid, Vec<TaskForgePullRequestRef>> {
    let mut grouped: HashMap<Uuid, Vec<TaskForgePullRequestRef>> = HashMap::new();
    for row in rows {
        grouped
            .entry(row.owner)
            .or_default()
            .push(TaskForgePullRequestRef {
                number: row.number,
                state: row.state,
                html_url: row.html_url,
            });
    }
    grouped
}

/// メッセージを先頭行と、2 行目以降（前後の空白を除く）に分ける
fn split_commit_message(message: &str) -> (String, String) {
    let (subject, body) = message.split_once('\n').unwrap_or((message, ""));
    (subject.trim_end().to_owned(), body.trim().to_owned())
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

    let pull_requests = task_pull_requests(&state, &auth, &task, None).await?;

    let backend = state.db.get_database_backend();
    // コミットを含む PR はまとめて 1 本で引く（コミットごとに引かない）
    let mut commit_pull_requests = group_pull_request_refs(
        PullRequestRefRow::find_by_statement(Statement::from_sql_and_values(
            backend,
            "SELECT pc.commit_id AS owner, p.number, p.state, p.html_url
             FROM forge_commit_links l
             JOIN forge_pull_request_commits pc ON pc.commit_id = l.commit_id
             JOIN forge_pull_requests p ON p.id = pc.pull_request_id
             WHERE l.task_id = $1
             ORDER BY p.number, p.id",
            [task.id.into()],
        ))
        .all(&state.db)
        .await?,
    );
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
    .map(|row| {
        let (message, body) = split_commit_message(&row.message);
        TaskForgeCommit {
            id: row.id,
            host: row.host,
            host_url: row.host_url,
            repo_owner: row.repo_owner,
            repo_name: row.repo_name,
            sha: row.sha,
            message,
            body,
            author_handle: row.author_handle,
            author_name: row.author_name,
            committed_at: row.committed_at,
            html_url: row.html_url,
            pull_requests: commit_pull_requests.remove(&row.id).unwrap_or_default(),
        }
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
        issue: service::forge::issue_source::find_for_task(&state.db, task.id).await?,
    }))
}

async fn task_branches(state: &AppState, task_id: Uuid) -> Result<Vec<TaskForgeBranch>, AppError> {
    // ブランチを head にしている PR はまとめて 1 本で引く。同じ名前のブランチは別のリポジトリにも
    // ありうるので、リポジトリまで一致させる。PR の送り先だけでなく head のリポジトリも見る
    // （フォークの同名ブランチからの PR を拾わない）。PR 行の持ち主はブランチのリンクと同じ連携のプロジェクトに限る
    let mut pull_requests = group_pull_request_refs(
        PullRequestRefRow::find_by_statement(Statement::from_sql_and_values(
            state.db.get_database_backend(),
            "SELECT b.id AS owner, p.number, p.state, p.html_url
             FROM forge_branch_links b
             JOIN forge_pull_requests p
               ON p.project_id = b.project_id
              AND p.host = b.host AND p.host_url = b.host_url
              AND p.repo_owner = b.repo_owner AND p.repo_name = b.repo_name
              AND p.head_branch = b.branch
              AND p.head_repo_owner = b.repo_owner AND p.head_repo_name = b.repo_name
             WHERE b.task_id = $1
             ORDER BY p.number, p.id",
            [task_id.into()],
        ))
        .all(&state.db)
        .await?,
    );
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
            pull_requests: pull_requests.remove(&row.id).unwrap_or_default(),
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
    state: &AppState,
    auth: &AuthUser,
    task: &tasks::Model,
    only: Option<Uuid>,
) -> Result<Vec<TaskForgePullRequest>, AppError> {
    let db = &state.db;
    // レビューの中身は review 系のスコープで守る（write:review は read:review を包含）
    let unresolved = service::reviews::blocking_by_task_pull_request(
        db,
        task.id,
        task.project_id,
        auth.require_scope(entity::scopes::Scope::ReadReview)
            .is_ok(),
    )
    .await?;
    // 題の末尾の `(KEY-N)` をこのタスクと照らすためのキー。行ごとに引かない
    let project_key = projects::Entity::find_by_id(task.project_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?
        .key;
    Ok(
        PullRequestRow::find_by_statement(Statement::from_sql_and_values(
            db.get_database_backend(),
            "SELECT p.id, p.project_id, p.host, p.host_url, p.repo_owner, p.repo_name, p.number,
                p.title, p.author_handle, p.state, p.html_url, p.merged_at, p.merge_effects_processed_at,
                p.commits_complete, p.head_branch,
                string_agg(l.source, ',' ORDER BY l.source) AS sources,
                bool_or(l.closes) AS closes,
                r.role AS role_override
         FROM forge_pull_requests p
         JOIN forge_pull_request_links l ON l.pull_request_id = p.id
         LEFT JOIN forge_pull_request_roles r ON r.pull_request_id = p.id AND r.task_id = $1
         WHERE l.task_id = $1 AND ($2::uuid IS NULL OR p.id = $2)
         GROUP BY p.id, r.role
         ORDER BY p.created_at DESC, p.id DESC",
            [task.id.into(), only.into()],
        ))
        .all(db)
        .await?
        .into_iter()
        .map(|row| {
            let sources: Vec<String> = row.sources.split(',').map(str::to_owned).collect();
            let role = pull_request_role(
                &row.title,
                &sources,
                row.closes,
                &project_key,
                task.seq_id,
                // 知らない値の行は上書きが無いものとして扱う（書く口は parse を通す）
                row.role_override.as_deref().and_then(PullRequestRole::parse),
            );
            TaskForgePullRequest {
                id: row.id,
                project_id: row.project_id,
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
                sources,
                closes: row.closes,
                role: role.as_str().to_owned(),
                head_branch: row.head_branch,
                unresolved: unresolved.get(&row.id).copied(),
            }
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
    let pull_request = task_pull_requests(&state, &auth, &task, Some(pull_request_id))
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
    put,
    path = "/{id}/forge/pull-requests/{pull_request_id}/role",
    tag = "Tasks",
    summary = "PR の役割（実装 / 言及）を手で決める",
    description = "題やリンクから決まる役割を上書きする。上書きは PR イベントでは戻らず、\
                   リンクを外すと一緒に消える。マージ時の自動完了（`closes`）は変えない。\
                   その PR がこのタスクにリンクされていなければ 404",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("pull_request_id" = Uuid, Path, description = "PR の ID"),
    ),
    request_body = SetTaskPullRequestRoleRequest,
    responses(
        (status = 200, description = "役割を変えた PR", body = TaskForgePullRequest),
        CrudErrors,
    )
)]
pub async fn set_task_pull_request_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, pull_request_id)): Path<(Uuid, Uuid, String, Uuid)>,
    Json(body): Json<SetTaskPullRequestRoleRequest>,
) -> Result<Json<TaskForgePullRequest>, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let role = PullRequestRole::parse(&body.role).ok_or(AppError::BadRequest)?;
    if !service::forge::pull_requests::set_role(
        &state.db,
        task.id,
        pull_request_id,
        role,
        auth.user_id,
    )
    .await?
    {
        return Err(AppError::NotFound);
    }
    // 直後に外されていれば行が無い。そのときは外した側が勝ったので 404
    let pull_request = task_pull_requests(&state, &auth, &task, Some(pull_request_id))
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    Ok(Json(pull_request))
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

#[cfg(test)]
mod tests {
    use super::split_commit_message;

    #[test]
    fn split_commit_message_separates_the_subject_from_the_trimmed_body() {
        for (message, subject, body) in [
            ("fix: x", "fix: x", ""),
            ("fix: x\n", "fix: x", ""),
            ("fix: x\n\n本文 1\n本文 2\n", "fix: x", "本文 1\n本文 2"),
            ("fix: x\r\n\r\n本文", "fix: x", "本文"),
            ("", "", ""),
        ] {
            assert_eq!(
                split_commit_message(message),
                (subject.to_owned(), body.to_owned()),
                "{message:?}"
            );
        }
    }
}

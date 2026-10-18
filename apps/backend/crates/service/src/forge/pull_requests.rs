//! PR イベントの共通処理（docs/features/tasks/9.github-tasks.md §5「PRイベント共通の前処理」）。
//!
//! どの action でも、適用するスナップショットを世代（ホスト側の更新時刻）で決め、PR 行の UPSERT と
//! `title` / `body` / `branch` 由来のリンク集合の再構築を 1 トランザクションで行う。
//! マージ時の副作用（タスク完了）と PR のコミット集合の同期はここでは行わない。

use std::future::Future;

use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Statement,
    TransactionTrait, prelude::Uuid,
};

use entity::{forge_pull_request_links, forge_pull_requests, projects};

use super::commits::can_view_project;
use super::events::ForgeRepo;
use super::task_refs::{self, ResolvedTaskRef};
use crate::task_activities::record_activity_once;

pub const PULL_REQUEST_LINKED_EVENT: &str = "forge_pull_request_linked";

/// PR のある時点の状態。webhook のペイロードか、ホストの PR 取得 API の結果から作る。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestSnapshot {
    pub title: String,
    pub body: String,
    pub author_handle: String,
    /// `open` / `closed`
    pub state: String,
    pub merged_at: Option<DateTime<Utc>>,
    pub head_sha: String,
    pub head_branch: String,
    pub html_url: String,
    pub updated_at: DateTime<Utc>,
}

/// PR イベントを現在状態へ反映する。適用したら `true`、古いイベントで何もしなかったら `false`。
///
/// `project_id` は webhook を受けた連携のプロジェクト（PR 行の持ち主）。`KEY-N` はそのテナント内で解決する。
/// イベントの更新時刻が既存行と同じときは、どちらが後か決められないので `fetch_current`
/// （ホストの PR 取得 API）で取り直したスナップショットを使う。取り直せなければ何も変えずに失敗する。
pub async fn apply_pull_request<F, Fut>(
    db: &DatabaseConnection,
    project_id: Uuid,
    repo: &ForgeRepo,
    number: i32,
    event: PullRequestSnapshot,
    fetch_current: F,
) -> Result<bool, anyhow::Error>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<PullRequestSnapshot, anyhow::Error>>,
{
    // 連携ごとプロジェクトが消えていれば、キーを解決するテナントも決まらない
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        return Ok(false);
    };

    let txn = db.begin().await?;
    // 行の有無にかかわらず自然キーで直列化する。行の id でロックすると、同じ PR を初めて作る
    // 並行ジョブが別々に比較して、古い方が後から COMMIT できてしまう
    txn.execute_raw(Statement::from_sql_and_values(
        txn.get_database_backend(),
        "SELECT pg_advisory_xact_lock(hashtext($1))",
        [format!(
            "forge_pull_request:{project_id}:{}:{}:{}:{}:{number}",
            repo.host, repo.host_url, repo.repo_owner, repo.repo_name
        )
        .into()],
    ))
    .await?;

    let known = forge_pull_requests::Entity::find()
        .filter(forge_pull_requests::Column::ProjectId.eq(project_id))
        .filter(forge_pull_requests::Column::Host.eq(&repo.host))
        .filter(forge_pull_requests::Column::HostUrl.eq(&repo.host_url))
        .filter(forge_pull_requests::Column::RepoOwner.eq(&repo.repo_owner))
        .filter(forge_pull_requests::Column::RepoName.eq(&repo.repo_name))
        .filter(forge_pull_requests::Column::Number.eq(number))
        .one(&txn)
        .await?
        .and_then(|row| row.host_updated_at)
        .map(|at| at.with_timezone(&Utc));
    let snapshot = match known {
        // 古いイベント。ロールバック（ドロップ）でロックも外れる
        Some(known) if event.updated_at < known => return Ok(false),
        // ホストの更新時刻は秒精度で、同じ秒の別 action はどちらが後か決められない
        Some(known) if event.updated_at == known => fetch_current().await?,
        _ => event,
    };

    let state = if snapshot.merged_at.is_some() {
        "merged"
    } else {
        snapshot.state.as_str()
    };
    let row = txn
        .query_one_raw(Statement::from_sql_and_values(
            txn.get_database_backend(),
            "INSERT INTO forge_pull_requests (id, project_id, host, host_url, repo_owner, repo_name,
                                              number, title, author_handle, state, head_sha, html_url,
                                              merged_at, host_updated_at, merge_effects_processed_at,
                                              merge_effects_acknowledged_by, commits_complete,
                                              created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NULL, NULL, false,
                     now(), now())
             ON CONFLICT (project_id, host, host_url, repo_owner, repo_name, number) DO UPDATE SET
                 title = EXCLUDED.title,
                 author_handle = EXCLUDED.author_handle,
                 -- マージは取り消せないので、open / closed のスナップショットで戻さない
                 state = CASE WHEN forge_pull_requests.state = 'merged' THEN 'merged'
                              ELSE EXCLUDED.state END,
                 head_sha = EXCLUDED.head_sha,
                 html_url = EXCLUDED.html_url,
                 merged_at = COALESCE(EXCLUDED.merged_at, forge_pull_requests.merged_at),
                 host_updated_at = EXCLUDED.host_updated_at,
                 updated_at = now()
             RETURNING id, state",
            [
                Uuid::new_v4().into(),
                project_id.into(),
                repo.host.clone().into(),
                repo.host_url.clone().into(),
                repo.repo_owner.clone().into(),
                repo.repo_name.clone().into(),
                number.into(),
                snapshot.title.clone().into(),
                snapshot.author_handle.clone().into(),
                state.into(),
                snapshot.head_sha.clone().into(),
                snapshot.html_url.clone().into(),
                snapshot.merged_at.into(),
                snapshot.updated_at.into(),
            ],
        ))
        .await?
        .ok_or_else(|| anyhow::anyhow!("upsert forge pull request returned no row"))?;
    let pull_request_id: Uuid = row.try_get("", "id")?;
    let state: String = row.try_get("", "state")?;

    // 参照元ごとにスナップショットの集合と一致させる（足りない行を足し、消えた行を消す）。`manual` は触らない
    let mut linked: Vec<ResolvedTaskRef> = Vec::new();
    for (source, text) in [
        ("title", &snapshot.title),
        ("body", &snapshot.body),
        ("branch", &snapshot.head_branch),
    ] {
        let targets =
            task_refs::resolve(&txn, project.tenant_id, &task_refs::extract(text)).await?;
        forge_pull_request_links::Entity::delete_many()
            .filter(forge_pull_request_links::Column::PullRequestId.eq(pull_request_id))
            .filter(forge_pull_request_links::Column::Source.eq(source))
            .filter(
                forge_pull_request_links::Column::TaskId
                    .is_not_in(targets.iter().map(|t| t.task_id)),
            )
            .exec(&txn)
            .await?;
        for target in &targets {
            txn.execute_raw(Statement::from_sql_and_values(
                txn.get_database_backend(),
                "INSERT INTO forge_pull_request_links (pull_request_id, task_id, closes, source, created_at)
                 VALUES ($1, $2, $3, $4, now())
                 ON CONFLICT (pull_request_id, task_id, source) DO UPDATE SET closes = EXCLUDED.closes",
                [
                    pull_request_id.into(),
                    target.task_id.into(),
                    target.closes.into(),
                    source.into(),
                ],
            ))
            .await?;
        }
        linked.extend(targets);
    }

    // 履歴は PR ↔ タスクにつき 1 回。タイトルと本文の両方に書かれていても 1 行
    linked.sort_by_key(|t| t.task_id);
    linked.dedup_by_key(|t| t.task_id);
    let author =
        super::identity::user_id_for_login(&txn, &repo.host, &snapshot.author_handle).await?;
    let payload = serde_json::json!({
        "host": repo.host,
        "number": number,
        "title": snapshot.title,
        "author_handle": snapshot.author_handle,
        "html_url": snapshot.html_url,
        "state": state,
    });
    for target in linked {
        let user_id = match author {
            Some(user_id)
                if can_view_project(&txn, project.tenant_id, target.project_id, user_id)
                    .await? =>
            {
                Some(user_id)
            }
            _ => None,
        };
        record_activity_once(
            &txn,
            target.task_id,
            user_id,
            PULL_REQUEST_LINKED_EVENT,
            payload.clone(),
            &format!(
                "{PULL_REQUEST_LINKED_EVENT}:{pull_request_id}:{}",
                target.task_id
            ),
        )
        .await?;
    }

    txn.commit().await?;
    Ok(true)
}

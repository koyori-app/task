//! push で届いたコミットをタスクへリンクし、アクティビティに積む
//! （docs/features/tasks/9.github-tasks.md §5 の `Push`）。
//!
//! 同じ push を再送・再試行で何度処理しても行が増えないよう、実体とリンクは UPSERT、
//! アクティビティは `dedupe_key` の UNIQUE で 1 回だけ積む。途中で失敗しても
//! 再試行で同じ状態に収束するので、コミットごとのトランザクションは張らない。

use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Statement, prelude::Uuid};

use entity::projects;

use super::events::{ForgeCommit, ForgeRepo};
use super::task_refs;
use crate::task_activities::record_activity_once;

pub const COMMIT_LINKED_EVENT: &str = "forge_commit_linked";

/// `project_id` は webhook を受けた連携のプロジェクト。`KEY-N` はそのテナント内で解決する。
pub async fn apply_push(
    db: &DatabaseConnection,
    project_id: Uuid,
    repo: &ForgeRepo,
    commits: &[ForgeCommit],
) -> Result<(), anyhow::Error> {
    // 連携ごとプロジェクトが消えていれば、キーを解決するテナントも決まらない
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        return Ok(());
    };

    for commit in commits {
        let refs = task_refs::extract(&commit.message);
        let targets = task_refs::resolve(db, project.tenant_id, &refs).await?;
        // ponytail: タスクに結ばないコミットは保存しない（読む口が無い）。PR のコミット集合を同期するときはそちらで実体を作る
        if targets.is_empty() {
            continue;
        }

        let commit_id = upsert_commit(db, project_id, repo, commit).await?;
        let payload = serde_json::json!({
            "host": repo.host,
            "sha": commit.sha,
            "message": commit.message.lines().next().unwrap_or_default(),
            "author_handle": commit.author_handle,
            "author_name": commit.author_name,
            "html_url": commit.html_url,
        });
        for target in targets {
            db.execute_raw(Statement::from_sql_and_values(
                db.get_database_backend(),
                "INSERT INTO forge_commit_links (commit_id, task_id, created_at)
                 VALUES ($1, $2, now())
                 ON CONFLICT DO NOTHING",
                [commit_id.into(), target.task_id.into()],
            ))
            .await?;
            record_activity_once(
                db,
                target.task_id,
                None,
                COMMIT_LINKED_EVENT,
                payload.clone(),
                &format!("{COMMIT_LINKED_EVENT}:{commit_id}:{}", target.task_id),
            )
            .await?;
        }
    }
    Ok(())
}

/// 同じリポジトリの同じ SHA は 1 行にまとめ、その行の id を返す。
async fn upsert_commit(
    db: &DatabaseConnection,
    project_id: Uuid,
    repo: &ForgeRepo,
    commit: &ForgeCommit,
) -> Result<Uuid, anyhow::Error> {
    // DO NOTHING だと競合時に RETURNING が行を返さないので、無害な更新で既存行の id を取る
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            db.get_database_backend(),
            "INSERT INTO forge_commits (id, project_id, host, host_url, repo_owner, repo_name,
                                        sha, message, author_handle, author_name, committed_at,
                                        html_url, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now())
             ON CONFLICT (project_id, host, host_url, repo_owner, repo_name, sha)
             DO UPDATE SET sha = EXCLUDED.sha
             RETURNING id",
            [
                Uuid::new_v4().into(),
                project_id.into(),
                repo.host.clone().into(),
                repo.host_url.clone().into(),
                repo.repo_owner.clone().into(),
                repo.repo_name.clone().into(),
                commit.sha.clone().into(),
                commit.message.clone().into(),
                commit.author_handle.clone().into(),
                commit.author_name.clone().into(),
                commit.committed_at.into(),
                commit.html_url.clone().into(),
            ],
        ))
        .await?
        .ok_or_else(|| anyhow::anyhow!("upsert forge commit returned no row"))?;
    Ok(row.try_get::<Uuid>("", "id")?)
}

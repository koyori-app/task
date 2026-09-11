//! push で届いたコミットをタスクへリンクし、アクティビティに積む
//! （docs/features/tasks/9.github-tasks.md §5 の `Push`）。
//!
//! 同じ push を再送・再試行で何度処理しても行が増えないよう、実体とリンクは UPSERT、
//! アクティビティは `dedupe_key` の UNIQUE で 1 回だけ積む。途中で失敗しても
//! 再試行で同じ状態に収束するので、コミットごとのトランザクションは張らない。

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect,
    Statement, prelude::Uuid,
};

use entity::{oauth_connections, projects, tenants};

use super::events::{ForgeCommit, ForgeRepo};
use super::task_refs;
use crate::access::project_accessible_user_ids;
use crate::task_activities::record_activity_once;

pub const COMMIT_LINKED_EVENT: &str = "forge_commit_linked";

/// `project_id` は webhook を受けた連携のプロジェクト。`KEY-N` はそのテナント内で解決する。
/// 作者がホスト上のログイン名で Task ユーザーに結べて、リンク先のプロジェクトに入れる人なら、
/// 履歴をそのユーザーの操作にする。
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

        let author = author_user_id(db, repo, &commit.author_handle).await?;
        let payload = serde_json::json!({
            "host": repo.host,
            "sha": commit.sha,
            "message": commit.message.lines().next().unwrap_or_default(),
            "author_handle": commit.author_handle,
            "author_name": commit.author_name,
            "html_url": commit.html_url,
        });
        for target in targets {
            // コミット行は受信した連携ではなくリンク先タスクのプロジェクトに置く。キーはテナント全体で
            // 解決するので、同じリポジトリを同じテナントの複数プロジェクトへ連携すると、連携ごとの
            // ジョブが同じタスクに届く。行を連携側に置くと commit_id が分かれてリンクも履歴も重複する
            let commit_id = upsert_commit(db, target.project_id, repo, commit).await?;
            db.execute_raw(Statement::from_sql_and_values(
                db.get_database_backend(),
                "INSERT INTO forge_commit_links (commit_id, task_id, created_at)
                 VALUES ($1, $2, now())
                 ON CONFLICT DO NOTHING",
                [commit_id.into(), target.task_id.into()],
            ))
            .await?;
            let user_id = match author {
                Some(user_id)
                    if can_view_project(db, project.tenant_id, target.project_id, user_id)
                        .await? =>
                {
                    Some(user_id)
                }
                _ => None,
            };
            record_activity_once(
                db,
                target.task_id,
                user_id,
                COMMIT_LINKED_EVENT,
                payload.clone(),
                &format!("{COMMIT_LINKED_EVENT}:{commit_id}:{}", target.task_id),
            )
            .await?;
        }
    }
    Ok(())
}

/// 作者を Task ユーザーに解決する。ホスト上のログイン名（小文字）を控えた接続がちょうど 1 件の
/// ときだけ採る。ログイン名の一意性は保存側の付け替えで保っているので、競合で 2 件あっても決め打ちしない。
///
/// `author_handle` はホストがコミットのメールアドレスから解決した値で、署名の無いコミットなら
/// 偽装できる。ここで得たユーザーは表示にだけ使い、通知や権限の根拠にしない。
async fn author_user_id(
    db: &DatabaseConnection,
    repo: &ForgeRepo,
    author_handle: &str,
) -> Result<Option<Uuid>, anyhow::Error> {
    if author_handle.is_empty() {
        return Ok(None);
    }
    // ponytail: クラウド版（instance_url が NULL）の接続だけを見る。セルフホストの GitLab / Forgejo を足すときは host_url と instance_url を突き合わせる
    let user_ids: Vec<Uuid> = oauth_connections::Entity::find()
        .filter(oauth_connections::Column::Provider.eq(repo.host.as_str()))
        .filter(oauth_connections::Column::InstanceUrl.is_null())
        .filter(oauth_connections::Column::ProviderLogin.eq(author_handle.to_lowercase()))
        .select_only()
        .column(oauth_connections::Column::UserId)
        .limit(2)
        .into_tuple()
        .all(db)
        .await?;
    Ok(match user_ids.as_slice() {
        [user_id] => Some(*user_id),
        _ => None,
    })
}

/// リンク先のプロジェクトに入れる人か（`require_project_access` と同じ規則。テナントオーナーは常に可）。
/// 入れない人の名前を、そのプロジェクトの履歴に載せないために見る。
async fn can_view_project(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<bool, anyhow::Error> {
    let is_owner = tenants::Entity::find_by_id(tenant_id)
        .one(db)
        .await?
        .is_some_and(|tenant| tenant.owner_id == user_id);
    Ok(is_owner
        || project_accessible_user_ids(db, project_id)
            .await?
            .contains(&user_id))
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

//! PR のコミット集合の同期（docs/features/tasks/9.github-tasks.md §5「PR のコミット集合の同期」）。
//!
//! `Opened` / `Synchronize` のたびにホストのコミット一覧を読み、`forge_pull_request_commits` を
//! その一覧と一致させる。一覧を読み切れなかったときは追加だけ行い、削除はしない
//! （短い一覧がそのまま削除の指示に化けるため）。

use std::future::Future;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Statement,
    TransactionTrait, prelude::Uuid,
};

use entity::{forge_pull_request_commits, forge_pull_requests};

use super::commits::{apply_push, upsert_commit};
use super::events::{ForgeCommit, ForgeRepo};

/// ホストのコミット一覧の 1 ページ。
#[derive(Debug)]
pub struct PullRequestCommitsPage {
    pub commits: Vec<ForgeCommit>,
    /// 続きがあるなら次に読むページ
    pub next_page: Option<u32>,
    /// ホストの上限に当たっていて、最後まで読んでも集合を確定できない（GitHub は 250 件）
    pub host_limit_reached: bool,
}

/// PR のコミット一覧を `fetch_page` で全ページ読み、中間表へ反映する。
///
/// `project_id` / `repo` / `number` は PR 行の自然キー。行が無ければ（プロジェクトごと消えた等）何もしない。
/// 途中のページで失敗したら、読めた分だけ反映して `commits_complete = false` を確定させてから
/// その失敗を返す（ジョブは再試行され、読み切れた時点で削除まで進む）。
pub async fn sync_pull_request_commits<F, Fut>(
    db: &DatabaseConnection,
    project_id: Uuid,
    repo: &ForgeRepo,
    number: i32,
    mut fetch_page: F,
) -> Result<(), anyhow::Error>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<PullRequestCommitsPage, anyhow::Error>>,
{
    let Some(pull_request) = forge_pull_requests::Entity::find()
        .filter(forge_pull_requests::Column::ProjectId.eq(project_id))
        .filter(forge_pull_requests::Column::Host.eq(&repo.host))
        .filter(forge_pull_requests::Column::HostUrl.eq(&repo.host_url))
        .filter(forge_pull_requests::Column::RepoOwner.eq(&repo.repo_owner))
        .filter(forge_pull_requests::Column::RepoName.eq(&repo.repo_name))
        .filter(forge_pull_requests::Column::Number.eq(number))
        .one(db)
        .await?
    else {
        return Ok(());
    };

    let txn = db.begin().await?;
    // 取得の前にロックする。反映時だけ取ると、古い一覧を先に取ったジョブが新しい一覧の反映後に上書きできる
    txn.execute_raw(Statement::from_sql_and_values(
        txn.get_database_backend(),
        "SELECT pg_advisory_xact_lock(hashtext($1))",
        [format!("forge_pull_request_commits:{}", pull_request.id).into()],
    ))
    .await?;

    let mut commits = Vec::new();
    let mut page = 1;
    let (complete, failure) = loop {
        match fetch_page(page).await {
            Ok(fetched) => {
                commits.extend(fetched.commits);
                match fetched.next_page {
                    Some(next) => page = next,
                    None => break (!fetched.host_limit_reached, None),
                }
            }
            Err(error) => break (false, Some(error)),
        }
    };

    // 1〜2: コミット実体とタスクへのリンク・アクティビティ（Push と同じ。リンク先タスクのプロジェクトに置く）
    apply_push(&txn, project_id, repo, &commits).await?;
    // 3: 中間表は PR 行と同じプロジェクトのコミット行に結ぶ（タスクに結ばないコミットもここで実体ができる）
    let mut commit_ids = Vec::with_capacity(commits.len());
    for commit in &commits {
        let commit_id = upsert_commit(&txn, project_id, repo, commit).await?;
        txn.execute_raw(Statement::from_sql_and_values(
            txn.get_database_backend(),
            "INSERT INTO forge_pull_request_commits (pull_request_id, commit_id, created_at)
             VALUES ($1, $2, now())
             ON CONFLICT DO NOTHING",
            [pull_request.id.into(), commit_id.into()],
        ))
        .await?;
        commit_ids.push(commit_id);
    }
    // 4: 読み切れたときだけ、この PR の行のうち一覧に無いものを消す（他の PR の行は触らない）
    if complete {
        forge_pull_request_commits::Entity::delete_many()
            .filter(forge_pull_request_commits::Column::PullRequestId.eq(pull_request.id))
            .filter(forge_pull_request_commits::Column::CommitId.is_not_in(commit_ids))
            .exec(&txn)
            .await?;
    }
    txn.execute_raw(Statement::from_sql_and_values(
        txn.get_database_backend(),
        "UPDATE forge_pull_requests SET commits_complete = $2 WHERE id = $1",
        [pull_request.id.into(), complete.into()],
    ))
    .await?;
    txn.commit().await?;

    match failure {
        Some(error) => Err(error.context(format!(
            "pull request commits page {page} could not be read"
        ))),
        None => Ok(()),
    }
}

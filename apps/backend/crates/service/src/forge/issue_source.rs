//! タスクの取り込み元 Issue（`GET /tasks/{id}/forge` の `issue`）。
//!
//! Issue のリンク表（`github_issue_links`）はまだ GitHub 専用なので、URL もここで組み立てる。
//! ホスト中立にするときに `host` / `host_url` / `html_url` の列を足して置き換える。

use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, prelude::Uuid};

use entity::{github_integrations, github_issue_links};
use payload::forge::TaskForgeIssue;

/// タスクの取り込み元 Issue。リンクが無ければ（連携解除で消えた場合も）`None`
pub async fn find_for_task<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
) -> Result<Option<TaskForgeIssue>, DbErr> {
    let Some((link, Some(integration))) = github_issue_links::Entity::find()
        .filter(github_issue_links::Column::TaskId.eq(task_id))
        .find_also_related(github_integrations::Entity)
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(TaskForgeIssue {
        host: "github".to_owned(),
        html_url: format!(
            "https://github.com/{}/{}/issues/{}",
            integration.repo_owner, integration.repo_name, link.github_number
        ),
        repo_owner: integration.repo_owner,
        repo_name: integration.repo_name,
        number: link.github_number,
        state: link.github_state,
        last_synced_at: link.updated_at.with_timezone(&chrono::Utc),
        pending_push: link.pending_push,
    }))
}

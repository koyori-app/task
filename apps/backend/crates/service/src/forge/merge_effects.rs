//! マージ時の副作用（docs/features/tasks/9.github-tasks.md §5「マージ時副作用の独立処理」）と、
//! 副作用を実行しないまま確認済みにする操作（§8 `merge-effects/acknowledge`）。
//!
//! どちらも PR 行をロックして `merge_effects_processed_at IS NULL` を確かめてから列を設定するので、
//! 再送・並行ジョブ・確認済み操作のどの組み合わせでも副作用は一度だけになる。

use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait, prelude::Uuid, sea_query::LockType,
};

use entity::{forge_pull_request_links, forge_pull_requests, project_statuses, projects, tasks};

use super::events::ForgeRepo;
use super::pull_requests::PullRequestSnapshot;
use super::task_refs;
use crate::error::AppError;
use crate::notifications::notify_status_changed;
use crate::task_activities::{record_activity, record_activity_once};

pub const PULL_REQUEST_MERGED_EVENT: &str = "forge_pull_request_merged";
pub const MERGE_EFFECTS_ACKNOWLEDGED_EVENT: &str = "forge_merge_effects_acknowledged";

/// `Closed { merged: true }` のイベントからマージ時の副作用（タスク完了）を一度だけ行う。
/// 今回処理したら `true`、処理済み・確認済み・PR 行が無いときは `false`。
///
/// 完了対象は現在のリンク表ではなく、イベントのタイトル・本文・head ブランチ（マージ時点の内容）と、
/// マージ前に作られて今も残る `manual` 行から決める。共通処理が古いイベントとして読み飛ばした場合も行う。
pub async fn apply_merge_effects(
    db: &DatabaseConnection,
    project_id: Uuid,
    repo: &ForgeRepo,
    number: i32,
    event: &PullRequestSnapshot,
) -> Result<bool, anyhow::Error> {
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        return Ok(false);
    };

    let txn = db.begin().await?;
    let Some(pull_request) = forge_pull_requests::Entity::find()
        .filter(forge_pull_requests::Column::ProjectId.eq(project_id))
        .filter(forge_pull_requests::Column::Host.eq(&repo.host))
        .filter(forge_pull_requests::Column::HostUrl.eq(&repo.host_url))
        .filter(forge_pull_requests::Column::RepoOwner.eq(&repo.repo_owner))
        .filter(forge_pull_requests::Column::RepoName.eq(&repo.repo_name))
        .filter(forge_pull_requests::Column::Number.eq(number))
        .lock(LockType::Update)
        .one(&txn)
        .await?
    else {
        return Ok(false);
    };
    // 並行ジョブの片方はロック待ちの後でここに来る。行ロック後に読み直した値で判定する
    if pull_request.merge_effects_processed_at.is_some() {
        return Ok(false);
    }
    let pull_request_id = pull_request.id;

    // task_id ごとの bool_or(closes)。BTreeMap の順でタスクをロックし、並行処理どうしのデッドロックを避ける
    let mut closes: BTreeMap<Uuid, bool> = BTreeMap::new();
    for text in [&event.title, &event.body, &event.head_branch] {
        for target in task_refs::resolve(&txn, project.tenant_id, &task_refs::extract(text)).await?
        {
            *closes.entry(target.task_id).or_default() |= target.closes;
        }
    }
    // マージ後に足した手動リンクは過去のマージの対象にしない
    if let Some(merged_at) = event.merged_at {
        for link in forge_pull_request_links::Entity::find()
            .filter(forge_pull_request_links::Column::PullRequestId.eq(pull_request_id))
            .filter(forge_pull_request_links::Column::Source.eq("manual"))
            .filter(forge_pull_request_links::Column::CreatedAt.lte(merged_at))
            .all(&txn)
            .await?
        {
            *closes.entry(link.task_id).or_default() |= link.closes;
        }
    }

    let payload = serde_json::json!({
        "host": repo.host,
        "number": number,
        "title": event.title,
        "author_handle": event.author_handle,
        "html_url": event.html_url,
        "state": "merged",
    });
    for task_id in closes
        .into_iter()
        .filter_map(|(task_id, closes)| closes.then_some(task_id))
    {
        let Some(task) = tasks::Entity::find_by_id(task_id)
            .filter(tasks::Column::DeletedAt.is_null())
            .lock(LockType::Update)
            .one(&txn)
            .await?
        else {
            continue;
        };
        let current = project_statuses::Entity::find_by_id(task.status_id)
            .one(&txn)
            .await?;
        if current.as_ref().is_some_and(|status| status.is_done_state) {
            continue;
        }
        let Some(done) = project_statuses::Entity::find()
            .filter(project_statuses::Column::ProjectId.eq(task.project_id))
            .filter(project_statuses::Column::IsDoneState.eq(true))
            .order_by_desc(project_statuses::Column::IsDefaultDone)
            .order_by_asc(project_statuses::Column::Position)
            .one(&txn)
            .await?
        else {
            tracing::warn!(
                %task_id,
                task_project_id = %task.project_id,
                %pull_request_id,
                "project has no done status; merged pull request leaves the task open"
            );
            continue;
        };

        let task_project_id = task.project_id;
        let from = current.map(|status| status.name).unwrap_or_default();
        let now = Utc::now();
        let completed_at = task.completed_at;
        let mut active: tasks::ActiveModel = task.into();
        active.status_id = Set(done.id);
        active.completed_at = Set(Some(completed_at.unwrap_or_else(|| now.into())));
        active.updated_at = Set(now.into());
        active.update(&txn).await?;

        record_activity_once(
            &txn,
            task_id,
            None,
            PULL_REQUEST_MERGED_EVENT,
            payload.clone(),
            &format!("{PULL_REQUEST_MERGED_EVENT}:{pull_request_id}:{task_id}"),
        )
        .await?;
        record_activity(
            &txn,
            task_id,
            None,
            "status_changed",
            serde_json::json!({ "from": from, "to": done.name }),
        )
        .await?;
        notify_status_changed(&txn, task_project_id, task_id, None, &from, &done.name).await?;
    }

    let mut active: forge_pull_requests::ActiveModel = pull_request.into();
    active.merge_effects_processed_at = Set(Some(Utc::now().into()));
    active.update(&txn).await?;
    txn.commit().await?;
    Ok(true)
}

/// マージ時の副作用を実行しないまま確認済みにする（§8）。タスクは完了させない。
///
/// パスのプロジェクトの PR でなければ 404、`merged` でないか処理済み・確認済みなら 409。
pub async fn acknowledge_merge_effects(
    db: &DatabaseConnection,
    project_id: Uuid,
    pull_request_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    let pull_request = forge_pull_requests::Entity::find_by_id(pull_request_id)
        .filter(forge_pull_requests::Column::ProjectId.eq(project_id))
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    if pull_request.state != "merged" || pull_request.merge_effects_processed_at.is_some() {
        return Err(AppError::Conflict);
    }

    let task_ids: Vec<Uuid> = forge_pull_request_links::Entity::find()
        .filter(forge_pull_request_links::Column::PullRequestId.eq(pull_request_id))
        .select_only()
        .column(forge_pull_request_links::Column::TaskId)
        .distinct()
        .into_tuple()
        .all(&txn)
        .await?;
    let payload = serde_json::json!({
        "host": pull_request.host,
        "pull_request_id": pull_request_id,
        "number": pull_request.number,
        "html_url": pull_request.html_url,
    });
    for task_id in task_ids {
        record_activity_once(
            &txn,
            task_id,
            Some(user_id),
            MERGE_EFFECTS_ACKNOWLEDGED_EVENT,
            payload.clone(),
            &format!("{MERGE_EFFECTS_ACKNOWLEDGED_EVENT}:{pull_request_id}:{task_id}"),
        )
        .await?;
    }

    let mut active: forge_pull_requests::ActiveModel = pull_request.into();
    active.merge_effects_processed_at = Set(Some(Utc::now().into()));
    active.merge_effects_acknowledged_by = Set(Some(user_id));
    active.update(&txn).await?;
    txn.commit().await?;
    Ok(())
}

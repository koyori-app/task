//! 担当者ごとの完了（`tasks.require_all_assignees_done`）。
//! docs/dev/features/tasks/1.core.md の「担当者」。
//!
//! 設定が on のタスクは、担当者全員が自分の分を完了にした時点で完了ステータスへ移る。
//! 完了ステータスへ直接移す経路は [`ensure_can_enter_done`] で止め、
//! 完了から戻す経路は [`reset_assignee_completions`] で全員の完了を消す。

use chrono::Utc;
use sea_orm::sea_query::{Expr, LockType};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Value, prelude::Uuid,
};

use crate::custom_fields::ensure_required_custom_fields;
use crate::error::AppError;
use crate::notifications::notify_status_changed;
use crate::task_activities::record_activity;
use entity::{project_statuses, task_assignees, tasks};
use payload::tasks::{ASSIGNEES_NOT_DONE, COMPLETION_NOT_REQUIRED, TASK_ALREADY_DONE};

/// 未完了（`completed_at IS NULL`）の担当者数。
pub async fn pending_assignee_count<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
) -> Result<u64, DbErr> {
    task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.eq(task_id))
        .filter(task_assignees::Column::CompletedAt.is_null())
        .count(db)
        .await
}

/// 未完了の担当者が残っていて、完了へ入れないタスクか。
/// 設定が off、または担当者 0 人なら false。完了へ入る経路の判定はすべてこれを使う。
pub async fn has_pending_assignees<C: ConnectionTrait>(
    db: &C,
    task: &tasks::Model,
) -> Result<bool, DbErr> {
    Ok(task.require_all_assignees_done && pending_assignee_count(db, task.id).await? > 0)
}

/// 未完了のタスクを完了ステータスへ直接移してよいか。
/// 設定が on で未完了の担当者が 1 人でもいれば 409 `assignees-not-done`。担当者 0 人は通す。
pub async fn ensure_can_enter_done<C: ConnectionTrait>(
    db: &C,
    task: &tasks::Model,
) -> Result<(), AppError> {
    if has_pending_assignees(db, task).await? {
        return Err(AppError::ConflictDetail(ASSIGNEES_NOT_DONE.into()));
    }
    Ok(())
}

/// タスクの担当者全員の `completed_at` を消す。
pub async fn reset_assignee_completions<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
) -> Result<(), DbErr> {
    task_assignees::Entity::update_many()
        .col_expr(
            task_assignees::Column::CompletedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .filter(task_assignees::Column::TaskId.eq(task_id))
        .filter(task_assignees::Column::CompletedAt.is_not_null())
        .exec(db)
        .await?;
    Ok(())
}

/// あるステータスにいる全タスクの行を `FOR UPDATE` で取る（`ORDER BY id` で取る順を固定する）。
/// 一括でタスクの `status_id` を書き換える前に担当者行を触る経路は、先にこれを呼んで
/// 「タスク行 → 担当者行」の順に揃える。
pub async fn lock_tasks_in_status<C: ConnectionTrait>(
    db: &C,
    status_id: Uuid,
) -> Result<(), DbErr> {
    common::db::execute_bound(
        db,
        "SELECT id FROM tasks WHERE status_id = $1 AND deleted_at IS NULL ORDER BY id FOR UPDATE",
        vec![Value::from(status_id)],
    )
    .await?;
    Ok(())
}

/// あるステータスにいる全タスクの担当者の完了を消す（ステータスの一括移行用）。
pub async fn reset_assignee_completions_in_status<C: ConnectionTrait>(
    db: &C,
    status_id: Uuid,
) -> Result<(), DbErr> {
    common::db::execute_bound(
        db,
        "UPDATE task_assignees SET completed_at = NULL WHERE completed_at IS NOT NULL \
         AND task_id IN (SELECT id FROM tasks WHERE status_id = $1 AND deleted_at IS NULL)",
        vec![Value::from(status_id)],
    )
    .await?;
    Ok(())
}

pub struct CompletionOutcome {
    pub task: tasks::Model,
    /// 取り込み元 Issue への書き戻しの印を立てた（呼び出し側がコミット後にジョブを積む）
    pub pending_push: bool,
}

/// タスク行を `FOR UPDATE` で取る。担当者・完了に触る経路は、必ず最初にこれを呼ぶ
/// （ロック順は「タスク行 → 担当者行」で統一する。逆順があるとデッドロックする）。
pub async fn lock_task<C: ConnectionTrait>(
    txn: &C,
    project_id: Uuid,
    task_id: Uuid,
) -> Result<tasks::Model, AppError> {
    tasks::Entity::find_by_id(task_id)
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .lock(LockType::Update)
        .one(txn)
        .await?
        .ok_or(AppError::NotFound)
}

/// タスクと本人の担当者行をロック付き（タスク行 → 担当者行の順）で取る。
/// 同時に 2 人が完了にしても、担当者の追加・削除と重なっても、このロックで直列になる。
/// 担当者が既に外されていれば 404。
async fn lock_task_and_assignee<C: ConnectionTrait>(
    txn: &C,
    project_id: Uuid,
    task_id: Uuid,
    user_id: Uuid,
) -> Result<(tasks::Model, task_assignees::Model), AppError> {
    let task = lock_task(txn, project_id, task_id).await?;
    if !task.require_all_assignees_done {
        return Err(AppError::ConflictDetail(COMPLETION_NOT_REQUIRED.into()));
    }
    let assignee = task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.eq(task_id))
        .filter(task_assignees::Column::UserId.eq(user_id))
        .lock(LockType::Update)
        .one(txn)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((task, assignee))
}

/// 自分の分を完了にする。最後の 1 人なら、既定の完了ステータスへ移す。
/// 呼び出し側がトランザクションを開いて commit する。
pub async fn complete_assignee<C: ConnectionTrait>(
    txn: &C,
    project_id: Uuid,
    task_id: Uuid,
    user_id: Uuid,
) -> Result<CompletionOutcome, AppError> {
    let (task, assignee) = lock_task_and_assignee(txn, project_id, task_id, user_id).await?;

    if assignee.completed_at.is_none() {
        let mut active: task_assignees::ActiveModel = assignee.into();
        active.completed_at = Set(Some(Utc::now().into()));
        active.update(txn).await?;
        record_activity(
            txn,
            task_id,
            Some(user_id),
            "assignee_completed",
            serde_json::json!({ "user_id": user_id }),
        )
        .await?;
    }

    let current = project_statuses::Entity::find_by_id(task.status_id)
        .one(txn)
        .await?;
    let already_done = current.as_ref().is_some_and(|s| s.is_done_state);
    if already_done || pending_assignee_count(txn, task_id).await? > 0 {
        return Ok(CompletionOutcome {
            task,
            pending_push: false,
        });
    }

    let done = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .filter(project_statuses::Column::IsDoneState.eq(true))
        .order_by_desc(project_statuses::Column::IsDefaultDone)
        .order_by_asc(project_statuses::Column::Position)
        .one(txn)
        .await?
        .ok_or(AppError::Conflict)?;
    // 直接移すときと同じ条件。未入力なら全体が巻き戻り、最後の人の完了も記録されない
    ensure_required_custom_fields(txn, project_id, task_id, None).await?;

    let from = current.map(|s| s.name).unwrap_or_default();
    let now = Utc::now();
    let completed_at = task.completed_at;
    let mut active: tasks::ActiveModel = task.into();
    active.status_id = Set(done.id);
    active.completed_at = Set(Some(completed_at.unwrap_or_else(|| now.into())));
    active.updated_at = Set(now.into());
    let task = active.update(txn).await?;
    record_activity(
        txn,
        task_id,
        Some(user_id),
        "status_changed",
        serde_json::json!({ "from": from, "to": done.name }),
    )
    .await?;
    notify_status_changed(txn, project_id, task_id, Some(user_id), &from, &done.name).await?;
    let pending_push = crate::github::sync::mark_pending_push(txn, task_id).await?;
    Ok(CompletionOutcome { task, pending_push })
}

/// 自分の分の完了を取り消す。タスクが完了済みの間は取り消せない（先にステータスを戻す）。
pub async fn reopen_assignee<C: ConnectionTrait>(
    txn: &C,
    project_id: Uuid,
    task_id: Uuid,
    user_id: Uuid,
) -> Result<tasks::Model, AppError> {
    let (task, assignee) = lock_task_and_assignee(txn, project_id, task_id, user_id).await?;

    let current = project_statuses::Entity::find_by_id(task.status_id)
        .one(txn)
        .await?;
    if current.is_some_and(|s| s.is_done_state) {
        return Err(AppError::ConflictDetail(TASK_ALREADY_DONE.into()));
    }

    if assignee.completed_at.is_some() {
        let mut active: task_assignees::ActiveModel = assignee.into();
        active.completed_at = Set(None);
        active.update(txn).await?;
        record_activity(
            txn,
            task_id,
            Some(user_id),
            "assignee_reopened",
            serde_json::json!({ "user_id": user_id }),
        )
        .await?;
    }
    Ok(task)
}

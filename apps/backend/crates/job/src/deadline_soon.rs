//! 期限の近いタスクを担当者とウォッチャーへ知らせるバッチ（`deadline_soon`。
//! `docs/features/tasks/5.notifications.md`）。
//!
//! 仮締め（`soft_deadline`）と Deadline（`hard_deadline`）を別々に見て、24 時間以内に
//! 入ったものを `payload.kind = "soft" | "hard"` の通知にする。同じ宛先・タスク・kind・期限の
//! 通知が既にあれば送らない（期限を変えると `payload.deadline` が変わるので送り直す）。
//! 複数インスタンスが同時に走るとまれに二重に入りうるが、DB の一意制約は足さずに許容する
//! （他の掃除ループと同じ割り切り）。

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use sea_orm::prelude::Uuid;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter};
use tokio::sync::watch;
use tracing::{info, warn};

use common::error::AppError;
use common::notifications::TYPE_DEADLINE_SOON;
use entity::{notifications, project_statuses, projects, task_assignees, task_watchers, tasks};
use service::notifications::{create_notification, in_app_enabled, notifiable_user_ids};

use crate::JobState;

/// 走らせる間隔。最初の 1 回は起動直後に走る（`tokio::time::interval` の初回 tick は即時）
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// 期限がこの長さ以内に入ったら知らせる
const WINDOW: chrono::Duration = chrono::Duration::hours(24);

/// 期限の近いタスクを 1 回走査して通知を作る。作った件数を返す。
pub async fn send_once(state: &JobState) -> Result<u64, AppError> {
    send_at(&state.db, Utc::now()).await
}

/// shutdown まで [`SWEEP_INTERVAL`] ごとに [`send_once`] を回す。
pub async fn run_sweeper(state: JobState, mut shutdown: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => match send_once(&state).await {
                Ok(0) => {}
                Ok(n) => info!(count = n, "deadline_soon notifications sent"),
                Err(e) => warn!(error = ?e, "deadline_soon sweep failed"),
            },
            _ = shutdown.changed() => break,
        }
    }
}

async fn send_at<C: ConnectionTrait>(db: &C, now: DateTime<Utc>) -> Result<u64, AppError> {
    let until = now + WINDOW;
    let in_window = |col: tasks::Column| Condition::all().add(col.gt(now)).add(col.lte(until));
    let due = tasks::Entity::find()
        .inner_join(project_statuses::Entity)
        .filter(project_statuses::Column::IsDoneState.eq(false))
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(tasks::Column::IsArchived.eq(false))
        .filter(
            Condition::any()
                .add(in_window(tasks::Column::SoftDeadline))
                .add(in_window(tasks::Column::HardDeadline)),
        )
        .find_also_related(projects::Entity)
        .all(db)
        .await?;

    let mut sent = 0;
    for (task, project) in due {
        let Some(project) = project else { continue };
        let notifiable = notifiable_user_ids(db, project.id).await?;
        let recipients: HashSet<_> = task_assignees::Entity::find()
            .filter(task_assignees::Column::TaskId.eq(task.id))
            .all(db)
            .await?
            .into_iter()
            .map(|row| row.user_id)
            .chain(
                task_watchers::Entity::find()
                    .filter(task_watchers::Column::TaskId.eq(task.id))
                    .all(db)
                    .await?
                    .into_iter()
                    .map(|row| row.user_id),
            )
            .filter(|user_id| notifiable.contains(user_id))
            .collect();

        for (kind, deadline) in [("soft", task.soft_deadline), ("hard", task.hard_deadline)] {
            let Some(deadline) = deadline.map(|d| d.with_timezone(&Utc)) else {
                continue;
            };
            if deadline <= now || deadline > until {
                continue;
            }
            let deadline = deadline.to_rfc3339_opts(SecondsFormat::AutoSi, true);
            for &user_id in &recipients {
                if already_sent(db, user_id, task.id, kind, &deadline).await?
                    || !in_app_enabled(db, user_id, project.id, TYPE_DEADLINE_SOON).await?
                {
                    continue;
                }
                create_notification(
                    db,
                    user_id,
                    Some(project.id),
                    Some(task.id),
                    TYPE_DEADLINE_SOON,
                    serde_json::json!({
                        "task_id": task.id,
                        "seq_key": format!("{}-{}", project.key, task.seq_id),
                        "title": task.title,
                        "kind": kind,
                        "deadline": deadline,
                    }),
                )
                .await?;
                sent += 1;
            }
        }
    }
    Ok(sent)
}

async fn already_sent<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    task_id: Uuid,
    kind: &str,
    deadline: &str,
) -> Result<bool, AppError> {
    Ok(notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(user_id))
        .filter(notifications::Column::TaskId.eq(task_id))
        .filter(notifications::Column::NotificationType.eq(TYPE_DEADLINE_SOON))
        .filter(Expr::cust_with_values(
            "payload ->> 'kind' = $1 AND payload ->> 'deadline' = $2",
            [kind, deadline],
        ))
        .one(db)
        .await?
        .is_some())
}

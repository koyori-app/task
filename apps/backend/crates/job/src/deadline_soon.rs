//! 期限の近いタスクを担当者とウォッチャーへ知らせるバッチ（`deadline_soon`。
//! `docs/dev/features/tasks/5.notifications.md`）。
//!
//! 仮締め（`soft_deadline`）と Deadline（`hard_deadline`）を別々に見て、24 時間以内に
//! 入ったものを `payload.kind = "soft" | "hard"` の通知にする。同じ宛先・タスク・kind・期限の
//! 通知が既にあれば送らない（期限を変えると `payload.deadline` が変わるので送り直す）。
//! 「確認してから作る」は排他でなく、起動直後の初回のように複数インスタンスが同時に走ると
//! 二重に入る。そこで Redis のロック（[`LOCK_KEY`]）を取れた 1 つだけが走査する。
//! DB の一意制約・部分インデックスは足さない（起動時の `sync()` と食い違い、起動できなくなる）。

use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use sea_orm::prelude::Uuid;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter};
use tokio::sync::watch;
use tracing::{info, warn};

use common::cache::redis::RedisConnection;
use common::error::AppError;
use common::notifications::TYPE_DEADLINE_SOON;
use entity::{notifications, project_statuses, projects, task_assignees, task_watchers, tasks};
use service::notifications::{create_notification, in_app_enabled, notifiable_user_ids};

use crate::JobState;

/// 走らせる間隔。最初の 1 回は起動直後に走る（`tokio::time::interval` の初回 tick は即時）
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// 期限がこの長さ以内に入ったら知らせる
const WINDOW: chrono::Duration = chrono::Duration::hours(24);

/// 走査を 1 インスタンスに絞るロック
pub const LOCK_KEY: &str = "deadline_soon:sweep_lock";

/// ロックの TTL。走査の途中で落ちても、次の回（[`SWEEP_INTERVAL`] 後）までには明ける
const LOCK_TTL_SECS: u64 = 10 * 60;

/// 取得時の値が現在値と一致するときだけ消す（TTL 切れの後に別インスタンスが取ったロックを消さない）
static RELEASE_LOCK_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        if redis.call('GET', KEYS[1]) == ARGV[1] then
            return redis.call('DEL', KEYS[1])
        end
        return 0
        "#,
    )
});

/// ロックを取って期限の近いタスクを 1 回走査し、作った通知の件数を返す。
/// 他のインスタンスが走査中なら何もせず 0 を返す。
pub async fn send_once(state: &JobState) -> Result<u64, AppError> {
    let Some(token) = try_lock(&state.redis_client).await? else {
        return Ok(0);
    };
    let sent = send_at(&state.db, Utc::now()).await;
    let released = unlock(&state.redis_client, &token).await;
    // 走査の失敗を先に返す。解放の失敗は TTL で明けるが、握り潰さずに返す
    let sent = sent?;
    released?;
    Ok(sent)
}

/// `SET LOCK_KEY <uuid> NX EX LOCK_TTL_SECS`。取れたら uuid を返す。
async fn try_lock(redis: &RedisConnection) -> Result<Option<String>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let token = Uuid::new_v4().to_string();
    let acquired: Option<String> = redis::cmd("SET")
        .arg(LOCK_KEY)
        .arg(&token)
        .arg("NX")
        .arg("EX")
        .arg(LOCK_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET NX deadline_soon lock: {e}"))?;
    Ok(acquired.map(|_| token))
}

async fn unlock(redis: &RedisConnection, token: &str) -> Result<(), anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let _: i32 = RELEASE_LOCK_SCRIPT
        .key(LOCK_KEY)
        .arg(token)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis release deadline_soon lock script: {e}"))?;
    Ok(())
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

/// ロックを取らずに `now` 時点で 1 回走査する。作った件数を返す。
pub async fn send_at<C: ConnectionTrait>(db: &C, now: DateTime<Utc>) -> Result<u64, AppError> {
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

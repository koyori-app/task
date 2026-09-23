//! 保持期間を過ぎた通知を消す掃除ループ（`docs/features/tasks/5.notifications.md`）。
//!
//! 期間は `NOTIFICATION_RETENTION_DAYS`（既定 90 日）。メール送信待ちの行
//! （[`notification_email::pending_condition`]）は残す。消すと送る前に通知ごと消える。
//! 通知の削除は監査ログ・遷移履歴に影響しない。複数インスタンスで同時に走っても、
//! 同じ条件の DELETE が重なるだけで結果は変わらない。
//!
//! 定期処理の書き方はスタック内の他の掃除（`notification_email` / `webhook_delivery`）と
//! 揃えて tokio のループにする。

use std::time::Duration;

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use tokio::sync::watch;
use tracing::{info, warn};

use entity::notifications;

use crate::JobState;
use crate::notification_email;

/// 掃除の間隔。最初の 1 回は起動直後に走る（`tokio::time::interval` の初回 tick は即時）
pub const PURGE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// 保持期間を過ぎた通知を 1 回消す。消した件数を返す。
pub async fn purge_once(state: &JobState) -> Result<u64, sea_orm::DbErr> {
    let before =
        chrono::Utc::now() - chrono::Duration::days(state.settings.notification_retention_days);
    delete_older_than(&state.db, before).await
}

/// shutdown まで [`PURGE_INTERVAL`] ごとに [`purge_once`] を回す。
pub async fn run_sweeper(state: JobState, mut shutdown: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(PURGE_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => match purge_once(&state).await {
                Ok(0) => {}
                Ok(n) => info!(count = n, "old notifications purged"),
                Err(e) => warn!(error = %e, "notification retention purge failed"),
            },
            _ = shutdown.changed() => break,
        }
    }
}

/// `before` より古い通知を消す。メール送信待ちの行は残す。消した件数を返す。
pub async fn delete_older_than<C: ConnectionTrait>(
    db: &C,
    before: chrono::DateTime<chrono::Utc>,
) -> Result<u64, sea_orm::DbErr> {
    let res = notifications::Entity::delete_many()
        .filter(notifications::Column::CreatedAt.lt(before))
        .filter(notification_email::pending_condition().not())
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}

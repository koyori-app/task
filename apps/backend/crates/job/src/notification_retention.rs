//! 保持期間を過ぎた通知を消す日次ジョブ（`docs/features/tasks/5.notifications.md`）。
//!
//! 期間は `NOTIFICATION_RETENTION_DAYS`（既定 90 日）。メール送信待ちの行
//! （[`notification_email::pending_condition`]）は残す。消すと送る前に通知ごと消える。
//! 通知の削除は監査ログ・遷移履歴に影響しない。複数インスタンスで同時に走っても、
//! 同じ条件の DELETE が重なるだけで結果は変わらない。

use std::str::FromStr;

use apalis::prelude::{BoxDynError, Data};
use apalis_cron::{CronStream, Tick};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use entity::notifications;

use crate::JobState;
use crate::notification_email;

pub const WORKER_NAME: &str = "notification_retention";

/// 毎日 03:00 UTC（秒 分 時 日 月 曜日）
const SCHEDULE: &str = "0 0 3 * * *";

pub fn stream() -> CronStream<cron::Schedule, chrono::Utc> {
    CronStream::new(cron::Schedule::from_str(SCHEDULE).expect("valid cron expression"))
}

pub async fn process(_tick: Tick, state: Data<JobState>) -> Result<(), BoxDynError> {
    let before =
        chrono::Utc::now() - chrono::Duration::days(state.settings.notification_retention_days);
    let deleted = delete_older_than(&state.db, before).await?;
    tracing::info!(deleted, %before, "old notifications deleted");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_parses() {
        let _ = stream();
    }
}

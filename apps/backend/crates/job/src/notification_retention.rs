//! 保持期間を過ぎた通知を消す日次ジョブ（`docs/features/tasks/5.notifications.md`）。
//!
//! 期間は `NOTIFICATION_RETENTION_DAYS`（既定 90 日）。通知の削除は監査ログ・
//! 遷移履歴に影響しない。複数インスタンスで同時に走っても、同じ条件の DELETE が
//! 重なるだけで結果は変わらない。

use std::str::FromStr;

use apalis::prelude::{BoxDynError, Data};
use apalis_cron::{CronStream, Tick};

use crate::JobState;

pub const WORKER_NAME: &str = "notification_retention";

/// 毎日 03:00 UTC（秒 分 時 日 月 曜日）
const SCHEDULE: &str = "0 0 3 * * *";

pub fn stream() -> CronStream<cron::Schedule, chrono::Utc> {
    CronStream::new(cron::Schedule::from_str(SCHEDULE).expect("valid cron expression"))
}

pub async fn process(_tick: Tick, state: Data<JobState>) -> Result<(), BoxDynError> {
    let before =
        chrono::Utc::now() - chrono::Duration::days(state.settings.notification_retention_days);
    let deleted = service::notifications::delete_older_than(&state.db, before).await?;
    tracing::info!(deleted, %before, "old notifications deleted");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_parses() {
        let _ = stream();
    }
}

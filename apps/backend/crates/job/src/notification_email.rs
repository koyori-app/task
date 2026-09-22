//! 通知メールの掃き出しループ。
//!
//! 通知の行そのものが送信待ちの行列（outbox）で、`email_queued_at` が立っていて
//! `emailed_at` が NULL の行を拾って送る。立てるのは
//! `service::notifications::create_notification`（`notification_settings.email_events`）。
//!
//! ponytail: apalis ではなくループにしたのは、通知を作る service から apalis へは積めない
//! （依存の向きが service → job）ため。待ちが 1 周（30 秒 × 50 件）で捌けなくなったら
//! apalis のキューに載せ替える。

use std::time::Duration;

use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait,
    sea_query::{LockBehavior, LockType},
};
use tokio::sync::watch;
use tracing::{info, warn};

use entity::{notifications, projects, tasks, tenants, users};

use crate::JobState;

/// これを超えたら諦める。SMTP の恒久的な失敗（宛先不正など）で永久に再送しない
pub const MAX_ATTEMPTS: i16 = 5;
/// 掃き出しの間隔
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);
/// 1 周で拾う件数。トランザクションを長く持たないための上限
const BATCH_SIZE: u64 = 50;

/// 送信待ちを拾って送る。送信できた件数を返す。
///
/// `FOR UPDATE SKIP LOCKED` で取るので、複数インスタンスで走らせても同じ行を
/// 二重に送らない。1 行の失敗は他の行を止めない（試行回数だけ進めて次へ）。
pub async fn send_pending_once(state: &JobState) -> Result<usize, anyhow::Error> {
    let txn = state.db.begin().await?;
    let pending = notifications::Entity::find()
        .filter(notifications::Column::EmailQueuedAt.is_not_null())
        .filter(notifications::Column::EmailedAt.is_null())
        .filter(notifications::Column::EmailAttempts.lt(MAX_ATTEMPTS))
        .order_by_asc(notifications::Column::CreatedAt)
        .limit(BATCH_SIZE)
        .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
        .all(&txn)
        .await?;

    let mut sent = 0usize;
    for notification in pending {
        if send_one(&txn, state, &notification).await? {
            sent += 1;
        }
    }
    txn.commit().await?;
    Ok(sent)
}

/// 送信対象から外す（`email_queued_at` を戻す）。メール未認証の利用者や、
/// 宛先を組み立てられない通知は何度拾っても送れないので待ち行列から落とす。
async fn abandon<C: ConnectionTrait>(
    db: &C,
    notification: &notifications::Model,
    reason: &str,
) -> Result<(), anyhow::Error> {
    warn!(
        notification_id = %notification.id,
        reason,
        "skip notification email"
    );
    let mut active: notifications::ActiveModel = notification.clone().into();
    active.email_queued_at = Set(None);
    active.update(db).await?;
    Ok(())
}

/// 1 件送る。送れたら `true`。
///
/// DB の失敗は `?` で呼び出し元へ伝播する（掃き出し全体を止める）。SMTP の失敗だけは
/// 試行回数を進めて `false` を返し、他の行の送信を続ける。
async fn send_one<C: ConnectionTrait>(
    db: &C,
    state: &JobState,
    notification: &notifications::Model,
) -> Result<bool, anyhow::Error> {
    let Some(user) = users::Entity::find_by_id(notification.user_id)
        .one(db)
        .await?
    else {
        abandon(db, notification, "user not found").await?;
        return Ok(false);
    };
    // 未認証のアドレスへは送らない（本人のものか確かめられていない）
    if !user.email_verified {
        abandon(db, notification, "email not verified").await?;
        return Ok(false);
    }
    let Some(project_id) = notification.project_id else {
        abandon(db, notification, "notification has no project").await?;
        return Ok(false);
    };
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        abandon(db, notification, "project not found").await?;
        return Ok(false);
    };
    let Some(tenant) = tenants::Entity::find_by_id(project.tenant_id)
        .one(db)
        .await?
    else {
        abandon(db, notification, "tenant not found").await?;
        return Ok(false);
    };
    let task = match notification.task_id {
        Some(task_id) => tasks::Entity::find_by_id(task_id).one(db).await?,
        None => None,
    };

    let mail = service::notification_email::render(
        notification,
        task.as_ref(),
        &project,
        &tenant,
        &state.settings.email_verification_app_url,
    );
    let result = state
        .smtp_client
        .send_email(&user.email, &mail.subject, &mail.text, Some(&mail.html))
        .await
        .map_err(|e| anyhow::anyhow!("send notification email: {e}"));

    let mut active: notifications::ActiveModel = notification.clone().into();
    match result {
        Ok(()) => {
            active.emailed_at = Set(Some(chrono::Utc::now().into()));
            active.update(db).await?;
            Ok(true)
        }
        Err(error) => {
            let attempts = notification.email_attempts + 1;
            active.email_attempts = Set(attempts);
            active.update(db).await?;
            if attempts >= MAX_ATTEMPTS {
                warn!(notification_id = %notification.id, attempts, %error, "notification email gave up");
            } else {
                warn!(notification_id = %notification.id, attempts, %error, "notification email failed");
            }
            Ok(false)
        }
    }
}

/// 掃き出しを [`SWEEP_INTERVAL`] ごとに回す。shutdown で抜ける。
pub async fn run_sweeper(state: JobState, mut shutdown: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => match send_pending_once(&state).await {
                Ok(0) => {}
                Ok(n) => info!(count = n, "notification emails sent"),
                Err(e) => warn!(error = %e, "notification email sweep failed"),
            },
            _ = shutdown.changed() => break,
        }
    }
}

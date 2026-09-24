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
use uuid::Uuid;

use crate::JobState;

/// これを超えたら諦める。SMTP の恒久的な失敗（宛先不正など）で永久に再送しない
pub const MAX_ATTEMPTS: i16 = 5;
/// 掃き出しの間隔
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);
/// 1 周で拾う件数
const BATCH_SIZE: u64 = 50;

/// まだ送信対象の行（未送信・待ち・試行回数に余りがある）。
fn pending() -> sea_orm::Select<notifications::Entity> {
    notifications::Entity::find()
        .filter(notifications::Column::EmailQueuedAt.is_not_null())
        .filter(notifications::Column::EmailedAt.is_null())
        .filter(notifications::Column::EmailAttempts.lt(MAX_ATTEMPTS))
}

/// 再試行待ちも pending のまま残し、期限の来た行だけ送信する。
fn due() -> sea_orm::Select<notifications::Entity> {
    pending().filter(notifications::Column::EmailQueuedAt.lte(chrono::Utc::now()))
}

fn retry_delay(attempt: i16) -> chrono::Duration {
    chrono::Duration::seconds(match attempt {
        1 => 30,
        2 => 5 * 60,
        3 => 30 * 60,
        _ => 2 * 60 * 60,
    })
}

/// 送信待ちを拾って送る。送信できた件数を返す。
///
/// 対象の id をロック無しで最大 [`BATCH_SIZE`] 件取り、1 行ごとに短いトランザクションで
/// `FOR UPDATE SKIP LOCKED` を取り直して送り、その行の更新を commit する。1 通ごとに
/// 確定するので、後の行の失敗で送信済みの印が消えて再送されることはない。他インスタンスが
/// 掴んでいる行・その間に送られた行は取れないので飛ばす。
pub async fn send_pending_once(state: &JobState) -> Result<usize, anyhow::Error> {
    let ids: Vec<Uuid> = due()
        .select_only()
        .column(notifications::Column::Id)
        .order_by_asc(notifications::Column::CreatedAt)
        .limit(BATCH_SIZE)
        .into_tuple()
        .all(&state.db)
        .await?;

    let mut sent = 0usize;
    for id in ids {
        let txn = state.db.begin().await?;
        let Some(notification) = due()
            .filter(notifications::Column::Id.eq(id))
            .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
            .one(&txn)
            .await?
        else {
            continue;
        };
        if send_one(&txn, state, &notification).await? {
            sent += 1;
        }
        txn.commit().await?;
    }
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
            if attempts < MAX_ATTEMPTS {
                active.email_queued_at =
                    Set(Some((chrono::Utc::now() + retry_delay(attempts)).into()));
            }
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

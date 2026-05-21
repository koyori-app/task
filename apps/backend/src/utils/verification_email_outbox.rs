//! 認証メール送信の transactional outbox。
//!
//! 登録・再送は DB に outbox 行を書き込むだけにし、Redis トークン保存と SMTP は
//! バックグラウンドワーカーが再試行可能な形で実行する。

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect,
};
use sea_orm::sea_query::{LockBehavior, LockType};
use tracing::warn;
use uuid::Uuid;

use crate::entities::verification_email_outbox::{
    self, Entity as OutboxEntity, Model as OutboxModel, OutboxStatus,
};
use crate::utils::db::with_transaction;
use crate::utils::{email_verification, verification_email_delivery};
use crate::AppState;

pub const MAX_ATTEMPTS: i32 = 8;
const BATCH_SIZE: u64 = 16;
/// 未処理行があるときのポーリング間隔
const POLL_INTERVAL_ACTIVE: Duration = Duration::from_secs(2);
/// outbox が空のときのポーリング間隔（空 SELECT のログ・DB 負荷を抑える）
const POLL_INTERVAL_IDLE: Duration = Duration::from_secs(30);
/// processing のまま固まった行を再取得するまでの猶予
const STALE_PROCESSING: ChronoDuration = ChronoDuration::minutes(10);

/// ユーザー作成と同一トランザクション内で呼ぶ。
pub async fn enqueue<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    email: String,
    token: String,
) -> Result<(), sea_orm::DbErr> {
    let row = verification_email_outbox::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        email: Set(email),
        token: Set(Some(token)),
        status: Set(OutboxStatus::Pending),
        attempts: Set(0),
        last_error: Set(None),
        created_at: Set(Utc::now().into()),
        claimed_at: Set(None),
        sent_at: Set(None),
    };
    OutboxEntity::insert(row).exec(db).await?;
    Ok(())
}

/// 直近で enqueue した分をできるだけ早く処理する。
pub fn wake_worker(state: AppState) {
    tokio::spawn(async move {
        if let Err(e) = process_pending(&state).await {
            warn!("verification email outbox wake failed: {e:#}");
        }
    });
}

/// 起動時からポーリングで未送信行を処理する。
///
/// 各サイクルは「処理 → 待機」の順。再起動直後の pending を最大30秒待たせない。
pub async fn run_worker(state: AppState) {
    let mut idle;
    loop {
        match process_pending(&state).await {
            Ok(n) => idle = n == 0,
            Err(e) => {
                warn!("verification email outbox poll failed: {e:#}");
                idle = false;
            }
        }

        let delay = if idle {
            POLL_INTERVAL_IDLE
        } else {
            POLL_INTERVAL_ACTIVE
        };
        tokio::time::sleep(delay).await;
    }
}

/// 処理対象にした outbox 行数（0 ならキューは空）
pub async fn process_pending(state: &AppState) -> Result<usize, anyhow::Error> {
    let rows = claim_pending_rows(&state.db).await?;
    let n = rows.len();
    for row in rows {
        if let Err(e) = process_one(state, row).await {
            warn!("verification email outbox item failed: {e:#}");
        }
    }
    Ok(n)
}

/// `FOR UPDATE SKIP LOCKED` で行を確保し、同一トランザクション内で processing にする。
async fn claim_pending_rows(
    db: &sea_orm::DatabaseConnection,
) -> Result<Vec<OutboxModel>, anyhow::Error> {
    with_transaction(db, |txn| {
        Box::pin(async move {
            let stale_before: sea_orm::prelude::DateTimeWithTimeZone =
                (Utc::now() - STALE_PROCESSING).into();

            let claimable = Condition::any()
                .add(verification_email_outbox::Column::Status.eq(OutboxStatus::Pending))
                .add(
                    Condition::all()
                        .add(
                            verification_email_outbox::Column::Status
                                .eq(OutboxStatus::Processing),
                        )
                        .add(
                            Condition::any()
                                .add(
                                    verification_email_outbox::Column::ClaimedAt.lt(stale_before),
                                )
                                .add(verification_email_outbox::Column::ClaimedAt.is_null()),
                        ),
                );

            let rows = OutboxEntity::find()
                .filter(claimable)
                .filter(verification_email_outbox::Column::Attempts.lt(MAX_ATTEMPTS))
                .order_by_asc(verification_email_outbox::Column::CreatedAt)
                .limit(BATCH_SIZE)
                .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
                .all(txn)
                .await?;

            let now: sea_orm::prelude::DateTimeWithTimeZone = Utc::now().into();
            let mut claimed = Vec::with_capacity(rows.len());
            for row in rows {
                let mut active: verification_email_outbox::ActiveModel = row.into();
                active.status = Set(OutboxStatus::Processing);
                active.claimed_at = Set(Some(now));
                claimed.push(active.update(txn).await?);
            }
            Ok(claimed)
        })
    })
    .await
}

async fn process_one(state: &AppState, row: OutboxModel) -> Result<(), anyhow::Error> {
    let Some(token) = row.token.clone() else {
        mark_failed(&state.db, row.id, "missing token").await?;
        return Ok(());
    };

    let delivery = async {
        email_verification::store_token(&state.redis_client, row.user_id, &token).await?;
        verification_email_delivery::send_verification_email(
            &state.smtp_client,
            &row.email,
            &state.settings,
            &token,
        )
        .await
    }
    .await;

    match delivery {
        Ok(()) => {
            let mut active: verification_email_outbox::ActiveModel = row.into();
            active.status = Set(OutboxStatus::Sent);
            active.token = Set(None);
            active.claimed_at = Set(None);
            active.sent_at = Set(Some(Utc::now().into()));
            active.last_error = Set(None);
            active.update(&state.db).await?;
        }
        Err(e) => {
            let attempts = row.attempts + 1;
            let mut active: verification_email_outbox::ActiveModel = row.into();
            active.attempts = Set(attempts);
            active.last_error = Set(Some(e.to_string()));
            active.claimed_at = Set(None);
            if attempts >= MAX_ATTEMPTS {
                active.status = Set(OutboxStatus::Failed);
                active.token = Set(None);
            } else {
                active.status = Set(OutboxStatus::Pending);
            }
            active.update(&state.db).await?;
        }
    }

    Ok(())
}

async fn mark_failed(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
    reason: &str,
) -> Result<(), anyhow::Error> {
    let row = OutboxEntity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("outbox row {id} not found"))?;
    let mut active: verification_email_outbox::ActiveModel = row.into();
    active.status = Set(OutboxStatus::Failed);
    active.token = Set(None);
    active.claimed_at = Set(None);
    active.last_error = Set(Some(reason.to_string()));
    active.update(db).await?;
    Ok(())
}

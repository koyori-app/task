//! Webhook 配信の掃き出しループ。
//!
//! 配信の行（`webhook_deliveries`）そのものが送信待ちの行列（outbox）で、
//! `next_attempt_at <= now()` の行を拾って送る。積むのは `service::webhooks::emit`。
//! 形は `job::notification_email` と同じ（1 行ごとに短いトランザクションで確定する）。

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
    sea_query::{Expr, ExprTrait, LockBehavior, LockType},
};
use tokio::sync::watch;
use tracing::{info, warn};
use uuid::Uuid;

use entity::{webhook_deliveries, webhooks};
use service::webhooks::{
    FORMAT_DISCORD, MAX_ATTEMPTS, MAX_FAILURE_STREAK, backoff, decrypt_secret, discord_message,
    sign, validate_url,
};

use crate::JobState;

/// 掃き出しの間隔。1 回目の試行は「即時」なので、遅延はこの間隔が上限になる
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(10);
/// 1 周で拾う件数
const BATCH_SIZE: u64 = 50;
/// 1 回の送信の上限
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// 配信履歴を残す期間（仕様 §5）
const RETENTION: chrono::Duration = chrono::Duration::days(90);
/// 古い履歴の掃除の間隔
const PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// 送信用のクライアント。リダイレクトは追わない（検証済みの URL から private な宛先へ
/// 302 で飛ばされると SSRF の検証をすり抜ける）。共有の `http_client` は追う設定なので分ける。
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("koyori-webhook")
        .redirect(reqwest::redirect::Policy::none())
        .timeout(SEND_TIMEOUT)
        // 送信先はまちまちで間隔も空くので、接続を持ち越さない
        .pool_max_idle_per_host(0)
        .build()
        .expect("build webhook http client")
});

fn due() -> sea_orm::Select<webhook_deliveries::Entity> {
    webhook_deliveries::Entity::find()
        .filter(webhook_deliveries::Column::NextAttemptAt.lte(chrono::Utc::now()))
}

/// 期限の来た配信を拾って送る。送信に成功した件数を返す。
///
/// 対象の id をロック無しで最大 [`BATCH_SIZE`] 件取り、1 行ごとに短いトランザクションで
/// `FOR UPDATE SKIP LOCKED` を取り直して送り、その行の更新を commit する。
/// 他インスタンスが掴んでいる行・その間に済んだ行は取れないので飛ばす。
pub async fn send_pending_once(state: &JobState) -> Result<usize, anyhow::Error> {
    let ids: Vec<Uuid> = due()
        .select_only()
        .column(webhook_deliveries::Column::Id)
        .order_by_asc(webhook_deliveries::Column::NextAttemptAt)
        .limit(BATCH_SIZE)
        .into_tuple()
        .all(&state.db)
        .await?;

    let mut sent = 0usize;
    for id in ids {
        let txn = state.db.begin().await?;
        let Some(delivery) = due()
            .filter(webhook_deliveries::Column::Id.eq(id))
            .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
            .one(&txn)
            .await?
        else {
            continue;
        };
        let webhook = webhooks::Entity::find_by_id(delivery.webhook_id)
            .one(&txn)
            .await?
            .ok_or_else(|| anyhow::anyhow!("webhook {} not found", delivery.webhook_id))?;

        let mut active: webhook_deliveries::ActiveModel = delivery.clone().into();
        if !webhook.is_active {
            // 止めた Webhook へは送らない。待ち行列から落とす
            active.next_attempt_at = Set(None);
            active.last_error = Set(Some("webhook is inactive".into()));
            active.update(&txn).await?;
            txn.commit().await?;
            continue;
        }

        let result = send(state, &webhook, &delivery).await;
        let attempt = delivery.attempt + 1;
        active.attempt = Set(attempt);
        match result {
            Ok(status) if (200..300).contains(&status) => {
                active.status_code = Set(Some(i32::from(status)));
                active.delivered_at = Set(Some(chrono::Utc::now().into()));
                active.next_attempt_at = Set(None);
                active.last_error = Set(None);
                active.update(&txn).await?;
                if webhook.failure_streak != 0 {
                    webhooks::Entity::update_many()
                        .col_expr(webhooks::Column::FailureStreak, Expr::value(0i16))
                        .filter(webhooks::Column::Id.eq(webhook.id))
                        .exec(&txn)
                        .await?;
                }
                sent += 1;
            }
            other => {
                let (status, error) = match other {
                    Ok(status) => (Some(i32::from(status)), format!("HTTP {status}")),
                    Err(error) => (None, error),
                };
                warn!(delivery_id = %delivery.id, attempt, %error, "webhook delivery failed");
                active.status_code = Set(status);
                active.last_error = Set(Some(error));
                if attempt < MAX_ATTEMPTS {
                    let wait = chrono::Duration::from_std(backoff(attempt))?;
                    active.next_attempt_at = Set(Some((chrono::Utc::now() + wait).into()));
                    active.update(&txn).await?;
                } else {
                    active.next_attempt_at = Set(None);
                    active.update(&txn).await?;
                    give_up(&txn, webhook.id).await?;
                }
            }
        }
        txn.commit().await?;
    }
    Ok(sent)
}

/// 打ち止めを数え、[`MAX_FAILURE_STREAK`] 回続いたら Webhook を止める。
/// 並行する掃き出しと数え落とさないよう、読み書きを 1 文の UPDATE で行う。
async fn give_up<C: sea_orm::ConnectionTrait>(
    db: &C,
    webhook_id: Uuid,
) -> Result<(), anyhow::Error> {
    let streak = Expr::col(webhooks::Column::FailureStreak).add(1);
    webhooks::Entity::update_many()
        .col_expr(webhooks::Column::FailureStreak, streak.clone())
        .col_expr(
            webhooks::Column::IsActive,
            Expr::col(webhooks::Column::IsActive).and(streak.lt(MAX_FAILURE_STREAK)),
        )
        .filter(webhooks::Column::Id.eq(webhook_id))
        .exec(db)
        .await?;
    Ok(())
}

/// 1 回送る。応答のステータスコードか、送れなかった理由を返す。
async fn send(
    state: &JobState,
    webhook: &webhooks::Model,
    delivery: &webhook_deliveries::Model,
) -> Result<u16, String> {
    // DNS の向き先は登録後に変えられるので、送る直前にも確かめる
    validate_url(&state.settings, &webhook.url).map_err(|e| e.to_string())?;

    let mut request = CLIENT
        .post(&webhook.url)
        .header("Content-Type", "application/json")
        .header("X-Task-Event", &delivery.event)
        .header("X-Task-Delivery", delivery.id.to_string());
    let body = if webhook.format == FORMAT_DISCORD {
        // Discord は独自ヘッダを見ないので署名は付けない（URL 自体が秘密）
        serde_json::to_vec(&discord_message(&delivery.event, &delivery.payload))
    } else {
        serde_json::to_vec(&delivery.payload)
    }
    .map_err(|e| format!("serialize payload: {e}"))?;
    if webhook.format != FORMAT_DISCORD {
        let secret = decrypt_secret(&state.settings, &webhook.secret_enc)
            .map_err(|e| format!("decrypt secret: {e}"))?;
        request = request.header("X-Task-Signature", sign(&secret, &body));
    }

    let response = request
        .body(body)
        .send()
        .await
        .map_err(|e| format!("send: {}", e.without_url()))?;
    Ok(response.status().as_u16())
}

/// 90 日より古い配信履歴を消す。消した件数を返す。
pub async fn purge_old(state: &JobState) -> Result<u64, anyhow::Error> {
    let result = webhook_deliveries::Entity::delete_many()
        .filter(webhook_deliveries::Column::CreatedAt.lt(chrono::Utc::now() - RETENTION))
        .exec(&state.db)
        .await?;
    Ok(result.rows_affected)
}

/// 掃き出しを [`SWEEP_INTERVAL`] ごとに、履歴の掃除を [`PURGE_INTERVAL`] ごとに回す。
/// shutdown で抜ける。
pub async fn run_sweeper(state: JobState, mut shutdown: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_purge: Option<Instant> = None;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                match send_pending_once(&state).await {
                    Ok(0) => {}
                    Ok(n) => info!(count = n, "webhook deliveries sent"),
                    Err(e) => warn!(error = %e, "webhook delivery sweep failed"),
                }
                if last_purge.is_none_or(|at| at.elapsed() >= PURGE_INTERVAL) {
                    last_purge = Some(Instant::now());
                    match purge_old(&state).await {
                        Ok(0) => {}
                        Ok(n) => info!(count = n, "old webhook deliveries purged"),
                        Err(e) => warn!(error = %e, "webhook delivery purge failed"),
                    }
                }
            }
            _ = shutdown.changed() => break,
        }
    }
}

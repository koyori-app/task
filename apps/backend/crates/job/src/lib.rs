//! Apalis バックグラウンドジョブ

pub mod already_registered_email;
pub mod deadline_soon;
pub mod failure_report;
pub mod github_issue_sync;
pub mod github_webhook;
pub mod notification_email;
pub mod notification_retention;
pub mod password_reset_email;
pub mod review_summary;
pub mod supervise;
pub mod tenant_invitation_email;
pub mod verification_email;
pub mod webhook_delivery;

use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{BackendExt, BackoffConfig, PollWith, Strategy, Task, TaskBuilder};
use apalis_postgres::{PgPool, PostgresStorage, Pubsub};

use common::settings::Settings;

pub use already_registered_email::{AlreadyRegisteredEmailJob, AlreadyRegisteredEmailStorage};
pub use github_issue_sync::{GithubIssueSyncJob, GithubIssueSyncStorage};
pub use github_webhook::{GithubWebhookJob, GithubWebhookStorage};
pub use password_reset_email::{PasswordResetEmailJob, PasswordResetEmailStorage};
pub use review_summary::{ReviewSummaryJob, ReviewSummaryStorage};
pub use tenant_invitation_email::{TenantInvitationEmailJob, TenantInvitationEmailStorage};
pub use verification_email::{
    MAX_RETRIES, QUEUE_NAME, VerificationEmailJob, VerificationEmailStorage,
};

/// ワーカーが必要とする依存の束。
/// AppState（handler クレート）を受け取ると job → handler の循環になるため、
/// ワーカーは実際に使う要素だけをここから受け取る。
#[derive(Clone)]
pub struct JobState {
    pub settings: Settings,
    pub db: sea_orm::DatabaseConnection,
    pub redis_client: common::cache::redis::RedisConnection,
    pub smtp_client: service::smtp::SmtpClient,
    pub http_client: reqwest::Client,
    /// 要約更新ジョブが、自分の番でなかったときに積み直すために持つ。
    /// ワーカーへ `AppState` を渡すと job → handler の循環になるので、
    /// 必要な依存はここに足す
    pub review_summary_storage: Arc<review_summary::ReviewSummaryStorage>,
    /// マージでタスクを自動完了したとき、取り込み元 Issue への書き戻しを積むために持つ
    pub github_issue_sync_storage: Arc<github_issue_sync::GithubIssueSyncStorage>,
    /// 切り詰められた push の続きのジョブを、投入の鍵（受信記録）と同じトランザクションで
    /// 積むために持つ。storage 経由では別接続になり、鍵とジョブが一緒に確定しない。
    pub pg_pool: PgPool,
    /// 利用者の連携トークン（`oauth_connections`）を復号・更新するための設定。
    /// 要約ジョブがレビュワー本人の名義で PR を Approve するのに使う
    pub oauth_settings: service::oauth::OAuthSettings,
}

/// 積むジョブの最大試行回数。apalis-postgres 1.0.0-rc.8 までの既定（5）に合わせる。
/// 1.0.0-rc.9 以降の既定は 25 で、積む側が決めなければ、リトライの上限が 5 倍になる。
/// 使い切った後の行は `Failed` のまま残る（rc.8 までは `Killed`）。
pub const DEFAULT_MAX_ATTEMPTS: usize = 5;

/// ジョブを積む `Task`。最大試行回数を [`DEFAULT_MAX_ATTEMPTS`] に決める。
pub fn new_task<Args>(args: Args) -> Task<Args> {
    TaskBuilder::new(args)
        .max_attempts(DEFAULT_MAX_ATTEMPTS)
        .build()
}

/// [`new_task`] の、実行を `delay` だけ後ろへずらす版。
pub fn new_task_after<Args>(args: Args, delay: Duration) -> Task<Args> {
    TaskBuilder::new(args)
        .max_attempts(DEFAULT_MAX_ATTEMPTS)
        .run_after(delay)
        .build()
}

/// ワーカーの backend の型。storage に、INSERT の通知と定期の poll を束ねた戦略を足したもの。
pub type WorkerBackend<Args> = PollWith<PostgresStorage<Args>, Strategy>;

/// ワーカーが使う backend を組む。INSERT の通知（LISTEN）で即座に起き、通知を取りこぼしても
/// `poll_interval` を基準に（空振りが続けば伸ばしながら）poll で拾う。
/// 通知の接続は backend ごとに別に張るので、ワーカーの世代ごとに呼ぶ。
pub fn worker_backend<Args>(
    storage: &PostgresStorage<Args>,
    poll_interval: Duration,
) -> WorkerBackend<Args> {
    let strategy = Strategy::new()
        .stream(Pubsub::new(
            storage.pool().clone(),
            storage.config().queue.to_string(),
        ))
        .interval_with_backoff(poll_interval, BackoffConfig::default());
    storage.clone().poll_with_strategy(strategy)
}

pub async fn setup_pool(database_url: &str) -> Result<PgPool, anyhow::Error> {
    // 通常時の予算: ワーカー 7 本の LISTEN が 1 本ずつ常時占有し、残り 9 本を heartbeat・poll・
    // 孤児回収・ハンドラーからの投入が共有する。世代交代中は旧リスナーの接続が UNLISTEN まで残る
    Ok(sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .connect(database_url)
        .await?)
}

pub async fn setup_verification_email_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<VerificationEmailStorage>, anyhow::Error> {
    verification_email::setup(pool, settings).await
}

pub async fn setup_github_webhook_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<GithubWebhookStorage>, anyhow::Error> {
    github_webhook::setup(pool, settings).await
}

pub async fn setup_github_issue_sync_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<GithubIssueSyncStorage>, anyhow::Error> {
    github_issue_sync::setup(pool, settings).await
}

pub async fn setup_review_summary_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<ReviewSummaryStorage>, anyhow::Error> {
    review_summary::setup(pool, settings).await
}

pub async fn setup_password_reset_email_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<PasswordResetEmailStorage>, anyhow::Error> {
    password_reset_email::setup(pool, settings).await
}

pub async fn setup_already_registered_email_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<AlreadyRegisteredEmailStorage>, anyhow::Error> {
    already_registered_email::setup(pool, settings).await
}

pub async fn setup_tenant_invitation_email_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<TenantInvitationEmailStorage>, anyhow::Error> {
    tenant_invitation_email::setup(pool, settings).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_task_caps_attempts() {
        let task = new_task("job");
        assert_eq!(task.ctx().max_attempts(), Some(DEFAULT_MAX_ATTEMPTS));
    }

    #[test]
    fn delayed_task_caps_attempts() {
        let task = new_task_after("job", Duration::from_secs(30));
        assert_eq!(task.ctx().max_attempts(), Some(DEFAULT_MAX_ATTEMPTS));
    }
}

use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{BoxDynError, Data, TaskSink};
use apalis_postgres::{Config, PgPool, PostgresStorage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use common::settings::Settings;
use service::auth::generate_email_verification_token;
use service::{password_reset, password_reset_email_delivery, password_reset_log};

use crate::JobState;

pub const QUEUE_NAME: &str = "password_reset_email";

/// パスワードリセットメール送信ジョブ（トークンは Redis のみに保持）。
#[derive(Clone, Serialize, Deserialize)]
pub struct PasswordResetEmailJob {
    pub user_id: Uuid,
    pub email: String,
}

impl PasswordResetEmailJob {
    pub fn new(user_id: Uuid, email: String) -> Self {
        Self { user_id, email }
    }
}

pub type PasswordResetEmailStorage = PostgresStorage<PasswordResetEmailJob>;

pub fn build_storage(pool: &PgPool, _settings: &Settings) -> PasswordResetEmailStorage {
    PostgresStorage::new(pool).with_config(Config::default().queue(QUEUE_NAME))
}

/// ワーカーが使う backend。INSERT の通知（LISTEN）と、取りこぼしを拾う定期の poll を併用する。
/// ワーカーの世代ごとに作り直す（通知の接続は世代ごとに張る）。
pub fn worker_backend(
    storage: &PasswordResetEmailStorage,
) -> crate::WorkerBackend<PasswordResetEmailJob> {
    crate::worker_backend(storage, Duration::from_secs(2))
}

pub async fn setup(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<PasswordResetEmailStorage>, anyhow::Error> {
    PostgresStorage::setup(pool).await?;
    Ok(Arc::new(build_storage(pool, settings)))
}

pub async fn enqueue(
    storage: &PasswordResetEmailStorage,
    job: PasswordResetEmailJob,
) -> Result<(), anyhow::Error> {
    let mut storage = storage.clone();
    storage
        .push_task(crate::new_task(job))
        .await
        .map_err(|e| anyhow::anyhow!("push password reset job: {e}"))
}

pub async fn process(job: PasswordResetEmailJob, state: Data<JobState>) -> Result<(), BoxDynError> {
    let token = generate_email_verification_token();
    password_reset::store_token(&state.redis_client, job.user_id, &token).await?;
    password_reset_email_delivery::send_password_reset_email(
        &state.smtp_client,
        &job.email,
        &state.settings,
        &token,
    )
    .await?;
    password_reset_log::email_sent(job.user_id);
    Ok(())
}

pub fn worker_concurrency(settings: &Settings) -> usize {
    settings.password_reset_worker_concurrency.max(1)
}

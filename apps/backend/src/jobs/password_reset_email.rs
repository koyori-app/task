use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{
    BackoffConfig, BoxDynError, Data, IntervalStrategy, StrategyBuilder, TaskSink,
};
use apalis_postgres::{Config, JsonCodec, PostgresStorage, PgPool};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::utils::auth::generate_email_verification_token;
use crate::utils::{password_reset, password_reset_email_delivery, password_reset_log};
use crate::{AppState, settings::Settings};

pub const QUEUE_NAME: &str = "password_reset_email";
pub const MAX_RETRIES: usize = 8;

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

pub type PasswordResetEmailStorage = PostgresStorage<
    PasswordResetEmailJob,
    apalis_postgres::CompactType,
    JsonCodec<apalis_postgres::CompactType>,
    apalis_postgres::PgNotify,
>;

pub fn build_storage(pool: &PgPool, _settings: &Settings) -> PasswordResetEmailStorage {
    let config = Config::new(QUEUE_NAME).with_poll_interval(
        StrategyBuilder::new()
            .apply(
                IntervalStrategy::new(Duration::from_secs(2))
                    .with_backoff(BackoffConfig::default()),
            )
            .build(),
    );
    PostgresStorage::new_with_notify(pool, &config)
}

pub async fn setup(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<PasswordResetEmailStorage>, sqlx::Error> {
    PostgresStorage::setup(pool).await?;
    Ok(Arc::new(build_storage(pool, settings)))
}

pub async fn enqueue(
    storage: &PasswordResetEmailStorage,
    job: PasswordResetEmailJob,
) -> Result<(), anyhow::Error> {
    let mut storage = storage.clone();
    storage
        .push(job)
        .await
        .map_err(|e| anyhow::anyhow!("push password reset job: {e}"))
}

pub async fn process(job: PasswordResetEmailJob, state: Data<AppState>) -> Result<(), BoxDynError> {
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

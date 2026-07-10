//! 登録済みメールアドレスへの新規登録試行を通知するジョブ（Apalis + PostgreSQL）。
//!
//! #26: 新規登録時に既存メールアドレスでも未使用時と同一のレスポンスを返す
//! （メールアドレス列挙対策）。既存アカウント宛には確認メールの代わりに
//! この通知メールを送る。トークンは扱わないため Redis へのアクセスはない。

use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{
    BackoffConfig, BoxDynError, Data, IntervalStrategy, StrategyBuilder, TaskSink,
};
use apalis_postgres::{Config, JsonCodec, PgPool, PostgresStorage};
use serde::{Deserialize, Serialize};

use common::settings::Settings;
use service::already_registered_email_delivery;

use crate::JobState;

pub const QUEUE_NAME: &str = "already_registered_email";
pub const MAX_RETRIES: usize = 8;

/// 既に登録済みのメールアドレス宛に送る通知ジョブのペイロード。
/// トークン等の機微情報は含めない（`job::verification_email` と同様の規約）。
#[derive(Clone, Serialize, Deserialize)]
pub struct AlreadyRegisteredEmailJob {
    pub email: String,
}

impl AlreadyRegisteredEmailJob {
    pub fn new(email: String) -> Self {
        Self { email }
    }
}

pub type AlreadyRegisteredEmailStorage = PostgresStorage<
    AlreadyRegisteredEmailJob,
    apalis_postgres::CompactType,
    JsonCodec<apalis_postgres::CompactType>,
    apalis_postgres::PgNotify,
>;

pub fn build_storage(pool: &PgPool, _settings: &Settings) -> AlreadyRegisteredEmailStorage {
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
) -> Result<Arc<AlreadyRegisteredEmailStorage>, sqlx::Error> {
    PostgresStorage::setup(pool).await?;
    Ok(Arc::new(build_storage(pool, settings)))
}

pub async fn enqueue(
    storage: &AlreadyRegisteredEmailStorage,
    job: AlreadyRegisteredEmailJob,
) -> Result<(), anyhow::Error> {
    let mut storage = storage.clone();
    storage
        .push(job)
        .await
        .map_err(|e| anyhow::anyhow!("push already registered email job: {e}"))
}

pub async fn process(
    job: AlreadyRegisteredEmailJob,
    state: Data<JobState>,
) -> Result<(), BoxDynError> {
    already_registered_email_delivery::send_already_registered_email(
        &state.smtp_client,
        &job.email,
        &state.settings,
    )
    .await?;
    Ok(())
}

pub fn worker_concurrency(settings: &Settings) -> usize {
    settings.already_registered_email_worker_concurrency.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ジョブペイロードは Postgres の apalis.jobs に平文で永続化される。
    /// このジョブはトークンを扱わないが、フィールド追加時の回帰ガードとして
    /// 期待キー集合を固定しておく（機微情報が紛れ込んでいないことの確認込み）。
    #[test]
    fn payload_contains_no_sensitive_fields() {
        let job = AlreadyRegisteredEmailJob::new("user@example.com".into());
        let value = serde_json::to_value(&job).expect("serialize job");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("payload is a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["email"]);
    }
}

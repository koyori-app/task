//! 認証メール送信ジョブ（Apalis + PostgreSQL）

use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{
    BackoffConfig, BoxDynError, Data, IntervalStrategy, StrategyBuilder, TaskSink,
};
use apalis_postgres::{Config, JsonCodec, PgPool, PostgresStorage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use common::settings::Settings;
use service::auth::generate_email_verification_token;
use service::{email_verification, verification_email_delivery};

use crate::JobState;

pub const QUEUE_NAME: &str = "verification_email";
pub const MAX_RETRIES: usize = 8;

/// 認証メール送信ワーカーが処理する Apalis ジョブペイロード（トークンは Redis のみに保持）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationEmailJob {
    /// 認証対象ユーザーの ID。
    pub user_id: Uuid,
    /// 送信先メールアドレス（正規化済みを想定）。
    pub email: String,
}

impl VerificationEmailJob {
    pub fn new(user_id: Uuid, email: String) -> Self {
        Self { user_id, email }
    }
}

pub type VerificationEmailStorage = PostgresStorage<
    VerificationEmailJob,
    apalis_postgres::CompactType,
    JsonCodec<apalis_postgres::CompactType>,
    apalis_postgres::PgNotify,
>;

pub fn build_storage(pool: &PgPool, _settings: &Settings) -> VerificationEmailStorage {
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
) -> Result<Arc<VerificationEmailStorage>, anyhow::Error> {
    PostgresStorage::setup(pool).await?;
    Ok(Arc::new(build_storage(pool, settings)))
}

pub async fn enqueue(
    storage: &VerificationEmailStorage,
    job: VerificationEmailJob,
) -> Result<(), anyhow::Error> {
    let mut storage = storage.clone();
    storage
        .push(job)
        .await
        .map_err(|e| anyhow::anyhow!("push verification email job: {e}"))?;
    Ok(())
}

/// 認証メールジョブを処理する（トークン生成 → Redis に保存 → SMTP 送信）。
///
/// トークンと発行世代（`issued_at`）はワーカー内で生成し、Redis のみに保持する。
/// 再送などでより新しい `issued_at` が既に Redis にある場合はトークン保存と送信を
/// スキップし、古いジョブの Apalis リトライが最新リンクを無効化しないようにする。
///
/// # Arguments
/// * `job` - 送信対象のユーザー ID・メールアドレス
/// * `state` - DB / Redis / SMTP などを含むアプリ状態
///
/// # Returns
/// * `Ok(())` - 送信成功、または世代が古くてスキップした場合（いずれもジョブ成功扱い）
///
/// # Errors
/// * Redis・SMTP など下位処理の失敗（Apalis がリトライする）
pub async fn process(job: VerificationEmailJob, state: Data<JobState>) -> Result<(), BoxDynError> {
    let token = generate_email_verification_token();
    let issued_at = chrono::Utc::now().timestamp_millis() as u64;
    let stored =
        email_verification::store_token(&state.redis_client, job.user_id, &token, issued_at)
            .await?;
    if !stored {
        // 再送などでより新しい世代が既に Redis にある。旧ジョブのリトライは送信しない。
        return Ok(());
    }
    verification_email_delivery::send_verification_email(
        &state.smtp_client,
        &job.email,
        &state.settings,
        &token,
    )
    .await?;
    Ok(())
}

pub fn worker_concurrency(settings: &Settings) -> usize {
    settings.verification_email_worker_concurrency.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ジョブペイロードは Postgres の apalis.jobs に平文で永続化されるため、
    /// 認証トークン等の機微情報を含めてはならない。型定義から `token` /
    /// `issued_at` を排除したこと（トークンは Redis のみに保持）の回帰ガード。
    /// フィールド追加でこのテストが落ちた場合は、機微情報でないことを
    /// 確認したうえで期待キー集合を更新すること。
    #[test]
    fn payload_contains_no_sensitive_fields() {
        let job = VerificationEmailJob::new(Uuid::new_v4(), "user@example.com".into());
        let value = serde_json::to_value(&job).expect("serialize job");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("payload is a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["email", "user_id"]);
    }
}

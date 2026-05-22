//! 認証メール送信ジョブ（Apalis + PostgreSQL）

use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{
    BackoffConfig, BoxDynError, Data, IntervalStrategy, StrategyBuilder, TaskSink,
};
use apalis_postgres::{Config, JsonCodec, PostgresStorage, PgPool};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::utils::{email_verification, verification_email_delivery};
use crate::{AppState, settings::Settings};

pub const QUEUE_NAME: &str = "verification_email";
pub const MAX_RETRIES: usize = 8;

/// 認証メール送信ワーカーが処理する Apalis ジョブペイロード。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationEmailJob {
    /// 認証対象ユーザーの ID。
    pub user_id: Uuid,
    /// 送信先メールアドレス（正規化済みを想定）。
    pub email: String,
    /// メール本文の認証リンクに埋め込むトークン。
    pub token: String,
    /// トークン発行世代（Unix ミリ秒）。[`email_verification::store_token`] は
    /// Redis 上の現世代より大きい値のときのみトークンを反映する。
    #[serde(default)]
    pub issued_at: u64,
}

impl VerificationEmailJob {
    /// キュー投入用のジョブを組み立て、`issued_at` に現在時刻（ミリ秒）を付与する。
    ///
    /// # Arguments
    /// * `user_id` - 認証対象ユーザー ID
    /// * `email` - 送信先メールアドレス
    /// * `token` - 認証トークン文字列
    ///
    /// # Returns
    /// * `issued_at` が設定された [`VerificationEmailJob`]
    pub fn new(user_id: Uuid, email: String, token: String) -> Self {
        Self {
            user_id,
            email,
            token,
            issued_at: chrono::Utc::now().timestamp_millis() as u64,
        }
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
) -> Result<Arc<VerificationEmailStorage>, sqlx::Error> {
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

/// 認証メールジョブを処理する（Redis にトークン保存 → SMTP 送信）。
///
/// 再送などでより新しい `issued_at` が既に Redis にある場合はトークン保存と送信を
/// スキップし、古いジョブの Apalis リトライが最新リンクを無効化しないようにする。
///
/// # Arguments
/// * `job` - 送信対象のユーザー・メール・トークン・発行世代
/// * `state` - DB / Redis / SMTP などを含むアプリ状態
///
/// # Returns
/// * `Ok(())` - 送信成功、または世代が古くてスキップした場合（いずれもジョブ成功扱い）
///
/// # Errors
/// * Redis・SMTP など下位処理の失敗（Apalis がリトライする）
pub async fn process(job: VerificationEmailJob, state: Data<AppState>) -> Result<(), BoxDynError> {
    let stored = email_verification::store_token(
        &state.redis_client,
        job.user_id,
        &job.token,
        job.issued_at,
    )
    .await?;
    if !stored {
        // 再送などでより新しい世代が既に Redis にある。旧ジョブのリトライは送信しない。
        return Ok(());
    }
    verification_email_delivery::send_verification_email(
        &state.smtp_client,
        &job.email,
        &state.settings,
        &job.token,
    )
    .await?;
    Ok(())
}

pub fn worker_concurrency(settings: &Settings) -> usize {
    settings.verification_email_worker_concurrency.max(1)
}

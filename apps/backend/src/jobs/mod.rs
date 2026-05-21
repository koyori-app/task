//! Apalis バックグラウンドジョブ

pub mod verification_email;

use std::sync::Arc;

use apalis_postgres::PgPool;

use crate::settings::Settings;

pub use verification_email::{
    VerificationEmailJob, VerificationEmailStorage, QUEUE_NAME, MAX_RETRIES,
};

pub async fn setup_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(database_url).await
}

pub async fn setup_verification_email_storage(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<VerificationEmailStorage>, sqlx::Error> {
    verification_email::setup(pool, settings).await
}

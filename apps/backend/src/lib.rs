use crate::{
    settings::Settings,
    utils::{
        oauth::OAuthSettings,
        redis::RedisConnection,
        smtp::SmtpClient,
        drive::DriveConfig,
        storage::StorageBackend,
    },
};
use sea_orm::DatabaseConnection;

pub mod auth_helpers;
pub mod dto;
pub mod error;
pub mod entities;
pub mod jobs;
pub mod extractors;
pub mod handlers;
pub mod openapi;
pub mod routes;
pub mod server;
pub mod settings;
pub mod utils;
pub mod middlewares;

use std::sync::Arc;

use apalis_postgres::PgPool;

use crate::jobs::{GithubWebhookStorage, PasswordResetEmailStorage, VerificationEmailStorage};

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub db: DatabaseConnection,
    pub pg_pool: PgPool,
    pub redis_client: RedisConnection,
    pub smtp_client: SmtpClient,
    pub verification_email_storage: Arc<VerificationEmailStorage>,
    pub github_webhook_storage: Arc<GithubWebhookStorage>,
    pub password_reset_email_storage: Arc<PasswordResetEmailStorage>,
    pub storage: Arc<dyn StorageBackend>,
    pub drive_config: DriveConfig,
    pub oauth_settings: OAuthSettings,
    pub http_client: reqwest::Client,
}

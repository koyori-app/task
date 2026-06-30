use crate::{
    settings::Settings,
    utils::{
        drive::DriveConfig, oauth::OAuthSettings, redis::RedisConnection, smtp::SmtpClient,
        storage::StorageBackend,
    },
};
use sea_orm::DatabaseConnection;

pub mod auth_helpers;
pub mod error;
pub mod extractors;
pub mod handlers;
pub mod jobs;
pub mod middlewares;
pub mod openapi;
pub mod payload;
pub mod routes;
pub mod server;
pub mod settings;
pub mod utils;

use std::sync::Arc;

use apalis_postgres::PgPool;
use webauthn_rs::prelude::Webauthn;

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
    pub webauthn: Arc<Webauthn>,
}

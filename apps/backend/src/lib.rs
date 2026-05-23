use crate::{
    settings::Settings,
    utils::{redis::RedisConnection, smtp::SmtpClient},
};
use sea_orm::DatabaseConnection;

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

use crate::jobs::VerificationEmailStorage;

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub db: DatabaseConnection,
    pub pg_pool: PgPool,
    pub redis_client: RedisConnection,
    pub smtp_client: SmtpClient,
    pub verification_email_storage: Arc<VerificationEmailStorage>,
}

use deadpool_redis::{Config, Pool, Runtime};
use sea_orm::DatabaseConnection;

pub mod entities;
pub mod handlers;
pub mod routes;
pub mod server;
pub mod settings;
pub mod utils;

#[derive(Clone)]
pub struct RedisConnection {
    pub conn: Pool,
}

impl RedisConnection {
    pub fn new(url: &str) -> Self {
        let cfg = Config::from_url(url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();
        Self { conn: pool }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub redis_client: RedisConnection,
}

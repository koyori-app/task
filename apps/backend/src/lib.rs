use crate::{settings::Settings, utils::redis::RedisConnection};
use sea_orm::DatabaseConnection;

pub mod dto;
pub mod entities;
pub mod extractors;
pub mod handlers;
pub mod openapi;
pub mod routes;
pub mod server;
pub mod settings;
pub mod utils;

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub db: DatabaseConnection,
    pub redis_client: RedisConnection,
}

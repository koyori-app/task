use sea_orm::DatabaseConnection;

pub mod entities;
pub mod handlers;
pub mod routes;
pub mod server;
pub mod settings;
pub mod utils;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
}

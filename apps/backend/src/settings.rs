use config::{Config, Environment};
use serde::Deserialize;


#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database_url: String,
    pub redis_url: String,
    pub sentry_dsn: Option<String>,
}

pub fn load_settings() -> Settings {
    dotenvy::dotenv().ok();
    let settings = Config::builder()
    .add_source(Environment::default())
    .build()
    .unwrap();

    settings.try_deserialize().unwrap()
}

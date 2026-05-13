use config::{Config, Environment};
use serde::Deserialize;


#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database_url: String,
    pub redis_url: String,
    pub sentry_dsn: Option<String>,
    #[serde(default = "default_allow_origin")]
    pub allow_origin: String,
}

fn default_allow_origin() -> String {
    "http://localhost:3000".to_string()
}

pub fn load_settings() -> Settings {
    dotenvy::dotenv().ok();
    let settings = Config::builder()
    .add_source(Environment::default())
    .build()
    .unwrap();

    settings.try_deserialize().unwrap()
}

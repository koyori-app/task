use config::{Config, Environment};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Settings {
    pub database_url: String,
    pub redis_url: String,
    pub sentry_dsn: Option<String>,
    #[serde(default = "default_allow_origin")]
    pub allow_origin: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_from: String,
}

/// Default CORS allow origin used when no `allow_origin` setting is provided.
///
/// The function returns the fixed origin string `http://localhost:3000`.
///
/// # Examples
///
/// ```
/// let origin = default_allow_origin();
/// assert_eq!(origin, "http://localhost:3000");
/// ```
fn default_allow_origin() -> String {
    "http://localhost:3000".to_string()
}

pub fn load_settings() -> Result<Settings, anyhow::Error> {
    dotenvy::dotenv().ok();
    let settings = Config::builder()
        .add_source(Environment::default())
        .build()?;

    settings
        .try_deserialize()
        .map_err(|e| anyhow::anyhow!("failed to deserialize settings: {e}"))
}

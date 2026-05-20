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
    /// 認証メールに載せるリンクのベース URL（例: `http://localhost:3000`）。末尾に `/verify-email?token=…` を付与します。
    #[serde(default = "default_email_verification_app_url")]
    pub email_verification_app_url: String,
}

fn default_allow_origin() -> String {
    "http://localhost:3000".to_string()
}

fn default_email_verification_app_url() -> String {
    default_allow_origin()
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

use config::{Config, Environment};
use serde::Deserialize;
use validator::Validate;

/// GitHub App 連携に必要な設定（`GITHUB_APP_ID` 等が揃っているときのみ有効）。
#[derive(Clone, Deserialize, Validate)]
pub struct GithubAppSettings {
    #[validate(length(min = 1, message = "github_app_id is required"))]
    pub github_app_id: String,
    #[validate(length(min = 1, message = "github_app_private_key is required"))]
    pub github_app_private_key: String,
    #[validate(length(min = 1, message = "github_app_webhook_secret is required"))]
    pub github_app_webhook_secret: String,
    #[validate(length(min = 1, message = "github_app_name is required"))]
    pub github_app_name: String,
    #[validate(length(
        min = 32,
        message = "github_token_encryption_key must be at least 32 characters"
    ))]
    pub github_token_encryption_key: String,
    #[serde(default = "default_github_app_frontend_base_url")]
    pub github_app_frontend_base_url: String,
}

#[derive(Clone, Deserialize, Validate)]
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
    #[validate(length(min = 1, message = "email_verification_app_url is required"))]
    #[validate(custom(
        function = "validate_email_verification_app_url",
        message = "email_verification_app_url must be a valid http or https base URL"
    ))]
    pub email_verification_app_url: String,
    #[validate(range(
        min = 1,
        message = "verification_email_worker_concurrency must be >= 1"
    ))]
    #[serde(default = "default_verification_email_worker_concurrency")]
    pub verification_email_worker_concurrency: usize,
    #[validate(length(min = 32, message = "PERSONAL_TOKEN_SECRET must be at least 32 characters"))]
    pub personal_token_secret: String,
    /// 起動時に管理者昇格するユーザーのメールアドレス（管理者ゼロ時のみ有効）。
    pub bootstrap_admin_email: Option<String>,
    /// GitHub App 連携。`GITHUB_APP_ID` 未設定時は `None`（他機能は起動可能）。
    #[serde(default, skip_deserializing)]
    pub github_app: Option<GithubAppSettings>,
}

impl Settings {
    pub fn github_app_enabled(&self) -> bool {
        self.github_app.is_some()
    }

    pub fn require_github_app(&self) -> Result<&GithubAppSettings, crate::error::AppError> {
        self.github_app
            .as_ref()
            .ok_or(crate::error::AppError::BadRequest)
    }
}

fn default_github_app_frontend_base_url() -> String {
    "http://localhost:3000".to_string()
}

fn default_verification_email_worker_concurrency() -> usize {
    1
}

fn default_allow_origin() -> String {
    "http://localhost:3000".to_string()
}

fn github_app_enabled_from_env() -> bool {
    std::env::var("GITHUB_APP_ID")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// 環境変数から GitHub App 設定を読み込む（未設定時は `None`）。
fn load_github_app_settings(
    config: &Config,
) -> Result<Option<GithubAppSettings>, anyhow::Error> {
    if !github_app_enabled_from_env() {
        return Ok(None);
    }

    let mut gh: GithubAppSettings = config
        .clone()
        .try_deserialize()
        .map_err(|e| anyhow::anyhow!("failed to deserialize github app settings: {e}"))?;

    gh.github_app_private_key = gh.github_app_private_key.replace("\\n", "\n");

    if gh.github_app_frontend_base_url.is_empty() {
        gh.github_app_frontend_base_url = config
            .get_string("email_verification_app_url")
            .map_err(|e| {
                anyhow::anyhow!("failed to read email_verification_app_url for github redirect: {e}")
            })?;
    }

    gh.validate()
        .map_err(|e| anyhow::anyhow!("invalid github app settings: {e}"))?;

    Ok(Some(gh))
}

pub fn load_settings() -> Result<Settings, anyhow::Error> {
    dotenvy::dotenv().ok();
    let config = Config::builder()
        .add_source(Environment::default())
        .build()?;

    let github_app = load_github_app_settings(&config)?;

    let mut settings: Settings = config
        .try_deserialize()
        .map_err(|e| anyhow::anyhow!("failed to deserialize settings: {e}"))?;

    settings.github_app = github_app;

    settings
        .validate()
        .map_err(|e| anyhow::anyhow!("invalid settings: {e}"))?;

    Ok(settings)
}

/// 絶対 URL の http(s) ベースのみ許可（`http:/host` のような scheme 直後1スラッシュは拒否）。
fn validate_email_verification_app_url(raw: &str) -> Result<(), validator::ValidationError> {
    let url = raw.trim();
    if url.is_empty() {
        return Err(validator::ValidationError::new("required"));
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(validator::ValidationError::new("http_or_https"));
    }

    let parsed = url::Url::parse(url).map_err(|_| validator::ValidationError::new("url"))?;

    if parsed.cannot_be_a_base() {
        return Err(validator::ValidationError::new("not_absolute"));
    }

    let Some(host) = parsed.host_str() else {
        return Err(validator::ValidationError::new("host"));
    };

    if host.is_empty() {
        return Err(validator::ValidationError::new("host"));
    }

    let after_scheme = url
        .strip_prefix(parsed.scheme())
        .and_then(|s| s.strip_prefix(':'))
        .unwrap_or("");
    if !after_scheme.starts_with("//") {
        return Err(validator::ValidationError::new("authority"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_settings(email_url: &str) -> Settings {
        Settings {
            database_url: String::new(),
            redis_url: String::new(),
            sentry_dsn: None,
            allow_origin: String::new(),
            smtp_host: String::new(),
            smtp_port: 0,
            smtp_username: String::new(),
            smtp_password: String::new(),
            smtp_from: String::new(),
            email_verification_app_url: email_url.to_string(),
            verification_email_worker_concurrency: 1,
            personal_token_secret: "a".repeat(32),
            bootstrap_admin_email: None,
            github_app: Some(test_github_app_settings()),
        }
    }

    fn test_github_app_settings() -> GithubAppSettings {
        GithubAppSettings {
            github_app_id: "1".into(),
            github_app_private_key: test_github_private_key(),
            github_app_webhook_secret: "webhook-secret".into(),
            github_app_name: "task-app".into(),
            github_token_encryption_key: "b".repeat(32),
            github_app_frontend_base_url: "http://localhost:3000".into(),
        }
    }

    fn check(url: &str) -> bool {
        base_settings(url).validate().is_ok()
    }

    fn test_github_private_key() -> String {
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF8PbnGy0AHB7MbzgZryNTg3nX3W\nnQ4H+Yh6zpt+o0f+4v6mK8b0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n\n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n\n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n\n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n0n\nAgMBAAECggEABdummy\n-----END RSA PRIVATE KEY-----".into()
    }

    #[test]
    fn accepts_valid_base_urls() {
        assert!(check("http://localhost:3000"));
        assert!(check("https://app.example.com"));
    }

    #[test]
    fn rejects_single_slash_after_scheme() {
        assert!(!check("http:/localhost:3000"));
        assert!(!check("https:/example.com"));
    }

    #[test]
    fn rejects_missing_slashes() {
        assert!(!check("http:localhost:3000"));
    }
}

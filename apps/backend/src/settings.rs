use config::{Config, Environment};
use serde::Deserialize;
use validator::Validate;

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
    /// 認証メールに載せるリンクのベース URL（必須。例: `https://app.example.com`）。
    /// 末尾に `/verify-email?token=…` を付与する。未設定・不正な値では起動しない。
    #[validate(length(min = 1, message = "email_verification_app_url is required"))]
    #[validate(custom(
        function = "validate_email_verification_app_url",
        message = "email_verification_app_url must be a valid http or https base URL"
    ))]
    pub email_verification_app_url: String,
    /// 認証メール Apalis ワーカーの並列度
    #[validate(range(
        min = 1,
        message = "verification_email_worker_concurrency must be >= 1"
    ))]
    #[serde(default = "default_verification_email_worker_concurrency")]
    pub verification_email_worker_concurrency: usize,
    /// PAT の HMAC-SHA256 署名に使う秘密鍵。起動時に必須。32バイト以上（256ビット）が必要。
    #[validate(length(min = 32, message = "PERSONAL_TOKEN_SECRET must be at least 32 characters"))]
    pub personal_token_secret: String,
}

fn default_verification_email_worker_concurrency() -> usize {
    1
}

fn default_allow_origin() -> String {
    "http://localhost:3000".to_string()
}

pub fn load_settings() -> Result<Settings, anyhow::Error> {
    dotenvy::dotenv().ok();
    let settings = Config::builder()
        .add_source(Environment::default())
        .build()?;

    let settings: Settings = settings
        .try_deserialize()
        .map_err(|e| anyhow::anyhow!("failed to deserialize settings: {e}"))?;

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

    // `http:/localhost` は url クレートではパースできるがベース URL として不正
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

    // scheme 直後は `//` 必須（`http:/foo` を弾く）
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

    fn check(url: &str) -> bool {
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
            email_verification_app_url: url.to_string(),
            verification_email_worker_concurrency: 1,
            personal_token_secret: "a".repeat(32),
        }
        .validate()
        .is_ok()
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

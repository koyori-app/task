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
    /// GitHub App の Client ID。インストール時のユーザー認可（`code` の交換）に使う。
    #[validate(length(min = 1, message = "github_app_client_id is required"))]
    pub github_app_client_id: String,
    /// GitHub App の Client secret。
    #[validate(length(min = 1, message = "github_app_client_secret is required"))]
    pub github_app_client_secret: String,
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
    /// Sentry のパフォーマンス計測でトランザクションを送る割合（0.0〜1.0）。
    /// 未設定・空なら 0.0（送らない）。値は [`Settings::traces_sample_rate`] で読む。
    #[serde(default, deserialize_with = "deserialize_optional_f32")]
    #[validate(range(
        min = 0.0,
        max = 1.0,
        message = "sentry_traces_sample_rate must be between 0.0 and 1.0"
    ))]
    pub sentry_traces_sample_rate: Option<f32>,
    /// 実行環境の名前（`RUST_ENV`。例: `production`）。Sentry の environment に渡す。
    pub rust_env: Option<String>,
    #[serde(default = "default_allow_origin")]
    pub allow_origin: String,
    /// HTTP サーバーの bind アドレス（例: `127.0.0.1:3400`）
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_from: String,
    /// アプリのベース URL（必須。例: `https://app.example.com`）。
    /// メール認証リンク・パスワードリセットリンク等すべてのメール送信で使用する。
    /// 未設定・不正な値では起動しない。
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
    /// パスワードリセットメール Apalis ワーカーの並列度
    #[validate(range(min = 1, message = "password_reset_worker_concurrency must be >= 1"))]
    #[serde(default = "default_password_reset_worker_concurrency")]
    pub password_reset_worker_concurrency: usize,
    /// GitHub Webhook Apalis ワーカーの並列度
    #[validate(range(min = 1, message = "github_webhook_worker_concurrency must be >= 1"))]
    #[serde(default = "default_github_webhook_worker_concurrency")]
    pub github_webhook_worker_concurrency: usize,
    /// 開発・テスト用に外部 Webhook の loopback 宛てを許可する（既定は拒否）。
    #[serde(default)]
    pub webhook_allow_loopback: bool,
    /// 登録済みメールアドレス通知（#26）Apalis ワーカーの並列度
    #[validate(range(
        min = 1,
        message = "already_registered_email_worker_concurrency must be >= 1"
    ))]
    #[serde(default = "default_already_registered_email_worker_concurrency")]
    pub already_registered_email_worker_concurrency: usize,
    /// 通知の保持日数。これより古い通知は日次ジョブが消す（メール送信待ちは残す）
    #[validate(range(min = 1, message = "notification_retention_days must be >= 1"))]
    #[serde(default = "default_notification_retention_days")]
    pub notification_retention_days: i64,
    /// PAT の HMAC-SHA256 署名に使う秘密鍵。起動時に必須。32バイト以上（256ビット）が必要。
    #[validate(length(
        min = 32,
        message = "PERSONAL_TOKEN_SECRET must be at least 32 characters"
    ))]
    pub personal_token_secret: String,
    /// リカバリーコード HMAC 用秘密鍵。PAT 秘密鍵とは分離。32 文字以上必須。
    #[validate(length(
        min = 32,
        message = "RECOVERY_CODE_SECRET must be at least 32 characters"
    ))]
    pub recovery_code_secret: String,
    /// TOTP シークレット暗号化用（AES-256-GCM）。UTF-8 で正確に 32 バイト必須。
    #[validate(custom(
        function = "validate_totp_encryption_key_bytes",
        message = "TOTP_ENCRYPTION_KEY must be exactly 32 bytes"
    ))]
    pub totp_encryption_key: String,
    /// otpauth URI の issuer（認証アプリ表示名）
    #[serde(default = "default_totp_issuer")]
    pub totp_issuer: String,
    /// 起動時に管理者昇格するユーザーのメールアドレス（管理者ゼロ時のみ有効）。
    pub bootstrap_admin_email: Option<String>,
    /// WebAuthn RP ID（省略時は `email_verification_app_url` のホスト名）
    pub webauthn_rp_id: Option<String>,
    /// 再認証の有効期間（秒）。パスキーの追加と削除は、直近のこの秒数の内に本人確認をしていることを求める。
    #[serde(default = "default_reauth_window_secs")]
    pub reauth_window_secs: u64,
    /// MCP の OAuth で受けるクライアントの `client_id`（URL）の許可リスト。環境変数
    /// `OAUTH_ALLOWED_CLIENT_IDS` の生の値（カンマ区切り）。読み方は
    /// [`Settings::oauth_allowed_clients`] の 1 箇所に置く。
    #[serde(default)]
    pub oauth_allowed_client_ids: String,
    /// GitHub App 連携。`GITHUB_APP_ID` 未設定時は `None`（他機能は起動可能）。
    #[serde(default, skip_deserializing)]
    pub github_app: Option<GithubAppSettings>,
}

/// `OAUTH_ALLOWED_CLIENT_IDS` が未設定・空のときに受ける `client_id`。
/// claude.ai のコネクタ、Claude Code、ChatGPT、Codex CLI の順。
pub const DEFAULT_OAUTH_ALLOWED_CLIENT_IDS: [&str; 4] = [
    "https://claude.ai/oauth/mcp-oauth-client-metadata",
    "https://claude.ai/oauth/claude-code-client-metadata",
    "https://chatgpt.com/oauth/client.json",
    "https://chatgpt.com/oauth/codex/client.json",
];

/// MCP の OAuth で受けるクライアント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowedClients {
    /// 任意の `client_id`（`*`）。
    Any,
    /// 完全一致で受ける `client_id`。空なら全部拒否する。
    List(Vec<String>),
}

impl AllowedClients {
    /// 完全一致の文字列比較。前方一致や正規化はしない。
    pub fn allows(&self, client_id: &str) -> bool {
        match self {
            AllowedClients::Any => true,
            AllowedClients::List(list) => list.iter().any(|allowed| allowed == client_id),
        }
    }
}

impl Settings {
    /// `oauth_allowed_client_ids` の読み方。
    ///
    /// - 未設定・空文字・空白だけ: 既定の 4 つ。本番の compose は未設定の変数を空文字で渡すので、
    ///   空を「全部拒否」とは読まない
    /// - `*` 1 つだけ: 任意の `client_id`
    /// - `none` 1 つだけ: 全部拒否
    /// - それ以外: カンマで割り、前後の空白と空の要素を落とした一覧。`*` や `none` がほかの値と
    ///   混ざっていたら、ただの文字列として扱う（どの URL とも一致しない）
    pub fn oauth_allowed_clients(&self) -> AllowedClients {
        let items: Vec<&str> = self
            .oauth_allowed_client_ids
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect();
        match items.as_slice() {
            [] => AllowedClients::List(
                DEFAULT_OAUTH_ALLOWED_CLIENT_IDS
                    .iter()
                    .map(|id| (*id).to_string())
                    .collect(),
            ),
            ["*"] => AllowedClients::Any,
            ["none"] => AllowedClients::List(Vec::new()),
            items => AllowedClients::List(items.iter().map(|id| (*id).to_string()).collect()),
        }
    }

    /// 公開の origin（`allow_origin` から末尾の `/` を落としたもの）。MCP の OAuth の issuer と
    /// 各エンドポイントの URL を作るのに使う。
    pub fn public_origin(&self) -> String {
        self.allow_origin.trim_end_matches('/').to_string()
    }

    pub fn traces_sample_rate(&self) -> f32 {
        self.sentry_traces_sample_rate.unwrap_or(0.0)
    }

    pub fn github_app_enabled(&self) -> bool {
        self.github_app.is_some()
    }

    pub fn require_github_app(&self) -> Result<&GithubAppSettings, crate::error::AppError> {
        self.github_app
            .as_ref()
            .ok_or(crate::error::AppError::NotFound)
    }
}

/// docker-compose の `${VAR:-}` は未設定を空文字で渡すので、空は未設定として読む。
fn deserialize_optional_f32<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => value.parse().map(Some).map_err(serde::de::Error::custom),
    }
}

fn default_github_app_frontend_base_url() -> String {
    String::new()
}

fn default_reauth_window_secs() -> u64 {
    300
}

fn default_totp_issuer() -> String {
    "TaskApp".to_string()
}

fn default_verification_email_worker_concurrency() -> usize {
    1
}

fn default_password_reset_worker_concurrency() -> usize {
    1
}

fn default_github_webhook_worker_concurrency() -> usize {
    1
}

fn default_already_registered_email_worker_concurrency() -> usize {
    1
}

fn default_notification_retention_days() -> i64 {
    90
}

fn default_allow_origin() -> String {
    "http://localhost:3000".to_string()
}

fn default_listen_addr() -> String {
    "0.0.0.0:3400".to_string()
}

/// AES-256 鍵素材は 32 バイト固定（マルチバイト文字の文字数ではなくバイト長で検証）。
fn validate_totp_encryption_key_bytes(raw: &str) -> Result<(), validator::ValidationError> {
    if raw.len() == 32 {
        Ok(())
    } else {
        Err(validator::ValidationError::new("totp_key_bytes"))
    }
}

fn github_app_enabled_from_env() -> bool {
    std::env::var("GITHUB_APP_ID")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// 環境変数から GitHub App 設定を読み込む（未設定時は `None`）。
fn load_github_app_settings(config: &Config) -> Result<Option<GithubAppSettings>, anyhow::Error> {
    if !github_app_enabled_from_env() {
        return Ok(None);
    }

    let mut gh: GithubAppSettings = config
        .clone()
        .try_deserialize()
        .map_err(|e| anyhow::anyhow!("failed to deserialize github app settings: {e}"))?;

    gh.github_app_private_key = gh.github_app_private_key.replace("\\n", "\n");

    if gh.github_app_frontend_base_url.is_empty() {
        gh.github_app_frontend_base_url =
            config
                .get_string("email_verification_app_url")
                .map_err(|e| {
                    anyhow::anyhow!(
                        "failed to read email_verification_app_url for github redirect: {e}"
                    )
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
            sentry_traces_sample_rate: None,
            rust_env: None,
            allow_origin: String::new(),
            listen_addr: default_listen_addr(),
            smtp_host: String::new(),
            smtp_port: 0,
            smtp_username: String::new(),
            smtp_password: String::new(),
            smtp_from: String::new(),
            email_verification_app_url: email_url.to_string(),
            verification_email_worker_concurrency: 1,
            password_reset_worker_concurrency: 1,
            github_webhook_worker_concurrency: 1,
            webhook_allow_loopback: false,
            already_registered_email_worker_concurrency: 1,
            notification_retention_days: 90,
            personal_token_secret: "a".repeat(32),
            recovery_code_secret: "c".repeat(32),
            totp_encryption_key: "b".repeat(32),
            totp_issuer: "TaskApp".to_string(),
            bootstrap_admin_email: None,
            webauthn_rp_id: None,
            reauth_window_secs: 300,
            oauth_allowed_client_ids: String::new(),
            github_app: Some(test_github_app_settings()),
        }
    }

    fn allowed(raw: &str) -> AllowedClients {
        let mut settings = base_settings("http://localhost:3000");
        settings.oauth_allowed_client_ids = raw.to_string();
        settings.oauth_allowed_clients()
    }

    fn list(items: &[&str]) -> AllowedClients {
        AllowedClients::List(items.iter().map(|item| (*item).to_string()).collect())
    }

    #[test]
    fn oauth_allowed_clients_defaults_to_the_four_known_clients() {
        let defaults = list(&[
            "https://claude.ai/oauth/mcp-oauth-client-metadata",
            "https://claude.ai/oauth/claude-code-client-metadata",
            "https://chatgpt.com/oauth/client.json",
            "https://chatgpt.com/oauth/codex/client.json",
        ]);
        for raw in ["", " ", "  \t ", ",", " , ,"] {
            assert_eq!(allowed(raw), defaults, "{raw:?}");
        }
        for id in DEFAULT_OAUTH_ALLOWED_CLIENT_IDS {
            assert!(defaults.allows(id), "{id}");
        }
        // 完全一致だけ。callback ID 付きの URL と末尾の `/` は通さない
        assert!(!defaults.allows("https://chatgpt.com/oauth/abc123/client.json"));
        assert!(!defaults.allows("https://chatgpt.com/oauth/client.json/"));
        assert!(!defaults.allows("https://claude.ai/oauth/"));
    }

    #[test]
    fn oauth_allowed_clients_reads_an_explicit_list() {
        // 1 件だけ書けばその 1 件。既定の 4 つは入らない
        let one = allowed("https://x.example/c.json");
        assert_eq!(one, list(&["https://x.example/c.json"]));
        assert!(!one.allows(DEFAULT_OAUTH_ALLOWED_CLIENT_IDS[0]));
        assert_eq!(allowed("a, ,b,"), list(&["a", "b"]));
        assert_eq!(allowed(" a ,b "), list(&["a", "b"]));
    }

    #[test]
    fn oauth_allowed_clients_reads_the_wildcard_and_none() {
        assert_eq!(allowed("*"), AllowedClients::Any);
        assert_eq!(allowed(" * "), AllowedClients::Any);
        assert!(allowed("*").allows("https://anything.example/c.json"));
        assert_eq!(allowed("none"), list(&[]));
        assert!(!allowed("none").allows(DEFAULT_OAUTH_ALLOWED_CLIENT_IDS[0]));
        // ほかの値と混ざった `*` と `none` は、ただの文字列
        let mixed = allowed("*,https://x.example/c.json");
        assert_eq!(mixed, list(&["*", "https://x.example/c.json"]));
        assert!(!mixed.allows("https://y.example/c.json"));
        assert_eq!(allowed("none,a"), list(&["none", "a"]));
    }

    #[test]
    fn public_origin_drops_the_trailing_slash() {
        let mut settings = base_settings("http://localhost:3000");
        settings.allow_origin = "https://app.example.com/".into();
        assert_eq!(settings.public_origin(), "https://app.example.com");
        settings.allow_origin = "https://app.example.com".into();
        assert_eq!(settings.public_origin(), "https://app.example.com");
    }

    fn test_github_app_settings() -> GithubAppSettings {
        GithubAppSettings {
            github_app_id: "1".into(),
            github_app_private_key: test_github_private_key(),
            github_app_webhook_secret: "webhook-secret".into(),
            github_app_name: "task-app".into(),
            github_app_client_id: "Iv1.testclientid".into(),
            github_app_client_secret: "test-client-secret".into(),
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
    fn traces_sample_rate_defaults_to_zero() {
        let settings = base_settings("http://localhost:3000");
        assert_eq!(settings.traces_sample_rate(), 0.0);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn traces_sample_rate_must_be_a_ratio() {
        let mut settings = base_settings("http://localhost:3000");
        settings.sentry_traces_sample_rate = Some(0.25);
        assert_eq!(settings.traces_sample_rate(), 0.25);
        assert!(settings.validate().is_ok());
        settings.sentry_traces_sample_rate = Some(1.5);
        assert!(settings.validate().is_err());
    }

    #[test]
    fn traces_sample_rate_reads_empty_as_unset() {
        #[derive(Deserialize)]
        struct Rate {
            #[serde(default, deserialize_with = "deserialize_optional_f32")]
            rate: Option<f32>,
        }
        let read = |value: &str| {
            Config::builder()
                .set_override("rate", value)
                .unwrap()
                .build()
                .unwrap()
                .try_deserialize::<Rate>()
        };
        assert_eq!(read("").unwrap().rate, None);
        assert_eq!(read("0.5").unwrap().rate, Some(0.5));
        assert!(read("often").is_err());
    }

    #[test]
    fn accepts_valid_base_urls() {
        assert!(check("http://localhost:3000"));
        assert!(check("https://app.example.com"));
    }

    #[test]
    fn rejects_urls_without_the_scheme_separator() {
        for url in [
            "http:/localhost:3000",
            "https:/example.com",
            "http:localhost:3000",
        ] {
            assert!(!check(url), "{url}");
        }
    }
}

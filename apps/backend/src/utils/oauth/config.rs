//! OAuth 設定（環境変数から読み込み）。

#[derive(Clone, Debug)]
pub struct OAuthSettings {
    /// OAuth コールバック URL のベース（例: `http://localhost:3400`）
    pub app_base_url: String,
    /// AES-256-GCM 暗号化キー（32 バイト）
    pub encryption_key: [u8; 32],
    /// ログイン後のデフォルトリダイレクト先（フロント相対パス）
    pub default_redirect_path: String,
    pub github: Option<ProviderConfig>,
    pub gitlab: Option<ProviderConfig>,
    pub gitlab_selfhosted: Option<ProviderConfig>,
    pub google: Option<ProviderConfig>,
    pub oidc: Option<OidcConfig>,
}

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, Debug)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
}

impl OAuthSettings {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        dotenvy::dotenv().ok();

        let app_base_url =
            env_var("APP_BASE_URL").unwrap_or_else(|| "http://localhost:3400".to_string());

        let github = pair_config(
            env_var("OAUTH_GITHUB_CLIENT_ID"),
            env_var("OAUTH_GITHUB_CLIENT_SECRET"),
        );
        let gitlab = pair_config(
            env_var("OAUTH_GITLAB_CLIENT_ID"),
            env_var("OAUTH_GITLAB_CLIENT_SECRET"),
        );
        let gitlab_selfhosted = pair_config(
            env_var("OAUTH_GITLAB_SELFHOSTED_CLIENT_ID"),
            env_var("OAUTH_GITLAB_SELFHOSTED_CLIENT_SECRET"),
        );
        let google = pair_config(
            env_var("OAUTH_GOOGLE_CLIENT_ID"),
            env_var("OAUTH_GOOGLE_CLIENT_SECRET"),
        );
        let oidc = match (
            env_var("OAUTH_OIDC_ISSUER_URL"),
            env_var("OAUTH_OIDC_CLIENT_ID"),
            env_var("OAUTH_OIDC_CLIENT_SECRET"),
        ) {
            (Some(issuer_url), Some(client_id), Some(client_secret)) => Some(OidcConfig {
                issuer_url: issuer_url.trim_end_matches('/').to_string(),
                client_id,
                client_secret,
            }),
            _ => None,
        };

        let has_providers = github.is_some()
            || gitlab.is_some()
            || gitlab_selfhosted.is_some()
            || google.is_some()
            || oidc.is_some();

        let encryption_key =
            parse_encryption_key(env_var("OAUTH_ENCRYPTION_KEY").as_deref(), has_providers)?;

        let default_redirect_path =
            env_var("OAUTH_DEFAULT_REDIRECT_PATH").unwrap_or_else(|| "/dashboard".to_string());

        Ok(Self {
            app_base_url: app_base_url.trim_end_matches('/').to_string(),
            encryption_key,
            default_redirect_path,
            github,
            gitlab,
            gitlab_selfhosted,
            google,
            oidc,
        })
    }

    pub fn callback_url(&self, provider_slug: &str) -> String {
        format!(
            "{}/v1/auth/oauth/{provider_slug}/callback",
            self.app_base_url
        )
    }

    pub fn is_provider_configured(&self, provider_slug: &str) -> bool {
        match provider_slug {
            "github" => self.github.is_some(),
            "gitlab" => self.gitlab.is_some(),
            "gitlab_selfhosted" => self.gitlab_selfhosted.is_some(),
            "google" => self.google.is_some(),
            "oidc" => self.oidc.is_some(),
            _ => false,
        }
    }

    pub fn has_any_provider(&self) -> bool {
        self.github.is_some()
            || self.gitlab.is_some()
            || self.gitlab_selfhosted.is_some()
            || self.google.is_some()
            || self.oidc.is_some()
    }

    /// DB に保存する provider キー（汎用 OIDC は `oidc:{issuer}`）。
    pub fn db_provider_key(&self, provider_slug: &str) -> Option<String> {
        match provider_slug {
            "github" | "gitlab" | "gitlab_selfhosted" | "google" => Some(provider_slug.to_string()),
            "oidc" => self.oidc.as_ref().map(|c| format!("oidc:{}", c.issuer_url)),
            _ => None,
        }
    }
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn pair_config(client_id: Option<String>, client_secret: Option<String>) -> Option<ProviderConfig> {
    match (client_id, client_secret) {
        (Some(id), Some(secret)) => Some(ProviderConfig {
            client_id: id,
            client_secret: secret,
        }),
        _ => None,
    }
}

fn parse_encryption_key(raw: Option<&str>, require: bool) -> Result<[u8; 32], anyhow::Error> {
    let Some(key_str) = raw.filter(|s| !s.is_empty()) else {
        if require {
            anyhow::bail!(
                "OAUTH_ENCRYPTION_KEY is required when at least one OAuth provider is configured"
            );
        }
        return Ok([0u8; 32]);
    };

    if key_str.len() < 32 {
        anyhow::bail!("OAUTH_ENCRYPTION_KEY must be at least 32 characters");
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&key_str.as_bytes()[..32]);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_key_optional_without_providers() {
        let key = parse_encryption_key(None, false).unwrap();
        assert_eq!(key, [0u8; 32]);
    }

    #[test]
    fn encryption_key_required_with_providers() {
        assert!(parse_encryption_key(None, true).is_err());
    }
}

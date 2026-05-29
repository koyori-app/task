//! OAuth プロバイダー定義と認可 URL 生成。

use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::config::{OAuthSettings, ProviderConfig};

#[derive(Debug, Clone)]
pub struct ProviderEndpoints {
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub scopes: Vec<&'static str>,
    pub use_oidc_id_token: bool,
}

#[derive(Debug, Clone)]
pub struct ProviderUserInfo {
    pub provider_user_id: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub username: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OidcDiscoveryDocument {
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    userinfo_endpoint: Option<String>,
}

/// OIDC Discovery（`.well-known/openid-configuration`）でエンドポイントを取得する。
pub async fn fetch_oidc_discovery(
    http: &Client,
    issuer_url: &str,
) -> Result<ProviderEndpoints, anyhow::Error> {
    let issuer = issuer_url.trim_end_matches('/');
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let doc: OidcDiscoveryDocument = http
        .get(&discovery_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let userinfo_url = doc
        .userinfo_endpoint
        .ok_or_else(|| anyhow::anyhow!("OIDC discovery document missing userinfo_endpoint"))?;

    Ok(ProviderEndpoints {
        authorize_url: doc.authorization_endpoint,
        token_url: doc.token_endpoint,
        userinfo_url,
        scopes: vec!["openid", "email", "profile"],
        use_oidc_id_token: true,
    })
}

pub async fn resolve_endpoints(
    provider_slug: &str,
    settings: &OAuthSettings,
    instance_url: Option<&str>,
    http: &Client,
) -> Result<ProviderEndpoints, anyhow::Error> {
    match provider_slug {
        "github" => Ok(ProviderEndpoints {
            authorize_url: "https://github.com/login/oauth/authorize".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            userinfo_url: "https://api.github.com/user".to_string(),
            scopes: vec!["read:user", "user:email"],
            use_oidc_id_token: false,
        }),
        "gitlab" => Ok(ProviderEndpoints {
            authorize_url: "https://gitlab.com/oauth/authorize".to_string(),
            token_url: "https://gitlab.com/oauth/token".to_string(),
            userinfo_url: "https://gitlab.com/api/v4/user".to_string(),
            scopes: vec!["read_user"],
            use_oidc_id_token: false,
        }),
        "gitlab_selfhosted" => {
            let base = instance_url
                .ok_or_else(|| anyhow::anyhow!("instance_url is required for gitlab_selfhosted"))?
                .trim_end_matches('/');
            validate_instance_url(base)?;
            Ok(ProviderEndpoints {
                authorize_url: format!("{base}/oauth/authorize"),
                token_url: format!("{base}/oauth/token"),
                userinfo_url: format!("{base}/api/v4/user"),
                scopes: vec!["read_user"],
                use_oidc_id_token: false,
            })
        }
        "google" => Ok(ProviderEndpoints {
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".to_string(),
            scopes: vec!["openid", "email", "profile"],
            use_oidc_id_token: true,
        }),
        "oidc" => {
            let issuer = settings
                .oidc
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("oidc provider not configured"))?
                .issuer_url
                .clone();
            fetch_oidc_discovery(http, &issuer).await
        }
        other => anyhow::bail!("unsupported oauth provider: {other}"),
    }
}

pub fn get_credentials(
    provider_slug: &str,
    settings: &OAuthSettings,
) -> Result<ProviderConfig, anyhow::Error> {
    match provider_slug {
        "github" => settings
            .github
            .clone()
            .ok_or_else(|| anyhow::anyhow!("github provider not configured")),
        "gitlab" => settings
            .gitlab
            .clone()
            .ok_or_else(|| anyhow::anyhow!("gitlab provider not configured")),
        "gitlab_selfhosted" => settings
            .gitlab_selfhosted
            .clone()
            .ok_or_else(|| anyhow::anyhow!("gitlab_selfhosted provider not configured")),
        "google" => settings
            .google
            .clone()
            .ok_or_else(|| anyhow::anyhow!("google provider not configured")),
        "oidc" => settings
            .oidc
            .as_ref()
            .map(|o| ProviderConfig {
                client_id: o.client_id.clone(),
                client_secret: o.client_secret.clone(),
            })
            .ok_or_else(|| anyhow::anyhow!("oidc provider not configured")),
        other => anyhow::bail!("unsupported oauth provider: {other}"),
    }
}

pub fn build_authorize_url(
    endpoints: &ProviderEndpoints,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    let scope = endpoints.scopes.join(" ");
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        endpoints.authorize_url,
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&scope),
        urlencoding::encode(state),
        urlencoding::encode(code_challenge),
    )
}

fn is_localhost_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn validate_instance_url(raw: &str) -> Result<(), anyhow::Error> {
    let parsed = Url::parse(raw)?;
    let scheme = parsed.scheme();
    if scheme != "https" && scheme != "http" {
        anyhow::bail!("instance_url must use http or https");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("instance_url must include a host"))?;
    if scheme == "http" && !is_localhost_host(host) {
        anyhow::bail!("instance_url over http is only allowed for localhost");
    }
    Ok(())
}

pub fn normalize_instance_url(raw: &str) -> Result<String, anyhow::Error> {
    let trimmed = raw.trim().trim_end_matches('/');
    validate_instance_url(trimmed)?;
    Ok(trimmed.to_string())
}

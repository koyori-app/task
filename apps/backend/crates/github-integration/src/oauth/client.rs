//! OAuth トークン交換・ユーザー情報取得 HTTP クライアント。

use chrono::{DateTime, Utc};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;

use super::config::ProviderConfig;
use super::provider::{ProviderEndpoints, ProviderUserInfo};

#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenJson {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    id: i64,
    login: String,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

#[derive(Debug, Deserialize)]
struct GitLabUser {
    id: i64,
    username: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    confirmed_at: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OidcUserInfo {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    picture: Option<String>,
}

pub async fn exchange_code(
    http: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    credentials: &ProviderConfig,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse, anyhow::Error> {
    let response = http
        .post(&endpoints.token_url)
        .header(ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", credentials.client_id.as_str()),
            ("client_secret", credentials.client_secret.as_str()),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await?
        .error_for_status()?;

    let token: OAuthTokenJson = response.json().await?;
    let expires_at = token
        .expires_in
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

    Ok(TokenResponse {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at,
    })
}

pub async fn fetch_user_info(
    http: &reqwest::Client,
    provider_slug: &str,
    endpoints: &ProviderEndpoints,
    access_token: &str,
) -> Result<ProviderUserInfo, anyhow::Error> {
    match provider_slug {
        "github" => fetch_github_user(http, access_token).await,
        "gitlab" | "gitlab_selfhosted" => {
            fetch_gitlab_user(http, &endpoints.userinfo_url, access_token).await
        }
        "google" | "oidc" => fetch_oidc_user(http, &endpoints.userinfo_url, access_token).await,
        other => anyhow::bail!("unsupported provider for userinfo: {other}"),
    }
}

async fn fetch_github_user(
    http: &reqwest::Client,
    access_token: &str,
) -> Result<ProviderUserInfo, anyhow::Error> {
    let user: GitHubUser = http
        .get("https://api.github.com/user")
        .headers(github_headers(access_token))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let (email, email_verified) = fetch_github_verified_email(http, access_token).await?;

    Ok(ProviderUserInfo {
        provider_user_id: user.id.to_string(),
        email,
        email_verified,
        username: user.login,
        avatar_url: user.avatar_url,
    })
}

async fn fetch_github_verified_email(
    http: &reqwest::Client,
    access_token: &str,
) -> Result<(Option<String>, Option<bool>), anyhow::Error> {
    let emails: Vec<GitHubEmail> = http
        .get("https://api.github.com/user/emails")
        .headers(github_headers(access_token))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let verified: Vec<&GitHubEmail> = emails.iter().filter(|e| e.verified).collect();
    let pick = verified
        .iter()
        .find(|e| e.primary)
        .copied()
        .or(verified.first().copied());

    Ok(match pick {
        Some(e) => (Some(e.email.clone()), Some(true)),
        None => (None, None),
    })
}

async fn fetch_gitlab_user(
    http: &reqwest::Client,
    userinfo_url: &str,
    access_token: &str,
) -> Result<ProviderUserInfo, anyhow::Error> {
    let user: GitLabUser = http
        .get(userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(ProviderUserInfo {
        provider_user_id: user.id.to_string(),
        email: user.email.clone(),
        email_verified: user.confirmed_at.as_ref().map(|_| true),
        username: user.username,
        avatar_url: user.avatar_url,
    })
}

async fn fetch_oidc_user(
    http: &reqwest::Client,
    userinfo_url: &str,
    access_token: &str,
) -> Result<ProviderUserInfo, anyhow::Error> {
    let user: OidcUserInfo = http
        .get(userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let username = user
        .preferred_username
        .or(user.name)
        .unwrap_or_else(|| user.sub.clone());

    Ok(ProviderUserInfo {
        provider_user_id: user.sub,
        email: user.email,
        email_verified: user.email_verified,
        username,
        avatar_url: user.picture,
    })
}

fn github_headers(access_token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}"))
            .unwrap_or_else(|_| HeaderValue::from_static("Bearer invalid")),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("task-oauth-backend"));
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick_github_email(emails: &[GitHubEmail]) -> (Option<String>, Option<bool>) {
        let verified: Vec<&GitHubEmail> = emails.iter().filter(|e| e.verified).collect();
        let pick = verified
            .iter()
            .find(|e| e.primary)
            .copied()
            .or(verified.first().copied());
        match pick {
            Some(e) => (Some(e.email.clone()), Some(true)),
            None => (None, None),
        }
    }

    #[test]
    fn github_email_uses_verified_primary_only() {
        let emails = vec![
            GitHubEmail {
                email: "unverified@example.com".into(),
                primary: true,
                verified: false,
            },
            GitHubEmail {
                email: "verified@example.com".into(),
                primary: false,
                verified: true,
            },
        ];
        let (email, verified) = pick_github_email(&emails);
        assert_eq!(email.as_deref(), Some("verified@example.com"));
        assert_eq!(verified, Some(true));
    }

    #[test]
    fn github_email_none_when_no_verified() {
        let emails = vec![GitHubEmail {
            email: "bad@example.com".into(),
            primary: true,
            verified: false,
        }];
        let (email, verified) = pick_github_email(&emails);
        assert!(email.is_none());
        assert!(verified.is_none());
    }
}

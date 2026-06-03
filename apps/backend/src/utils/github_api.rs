//! GitHub App JWT 発行と Installation Access Token 取得。

use std::sync::OnceLock;

use anyhow::{anyhow, Context};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::settings::GithubAppSettings;
use crate::utils::github_oauth_state::TTL_SECS;

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("build shared reqwest client")
    })
}

fn github_api_base() -> String {
    std::env::var("GITHUB_API_BASE_URL")
        .unwrap_or_else(|_| "https://api.github.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

#[derive(Debug, Serialize)]
struct AppJwtClaims {
    iss: String,
    iat: i64,
    exp: i64,
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse {
    token: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct RepositoryOwner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct InstallationRepository {
    pub full_name: String,
    pub owner: RepositoryOwner,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<InstallationRepository>,
}

#[derive(Debug, Deserialize)]
struct InstallationResponse {
    id: i64,
    account: RepositoryOwner,
    created_at: String,
}

#[derive(Debug, Clone)]
pub struct InstallationInfo {
    pub id: i64,
    pub account_login: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

pub struct InstallationAccessToken {
    pub token: String,
    pub expires_at: chrono::DateTime<chrono::FixedOffset>,
}

pub fn create_app_jwt(settings: &GithubAppSettings) -> Result<String, anyhow::Error> {
    let now = Utc::now();
    let claims = AppJwtClaims {
        iss: settings.github_app_id.clone(),
        iat: now.timestamp(),
        exp: (now + Duration::minutes(9)).timestamp(),
    };
    let key = EncodingKey::from_rsa_pem(settings.github_app_private_key.as_bytes())
        .context("parse github app private key PEM")?;
    encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &key,
    )
    .context("encode github app jwt")
}

pub async fn fetch_installation_access_token(
    settings: &GithubAppSettings,
    installation_id: i64,
) -> Result<InstallationAccessToken, anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!(
        "{}/app/installations/{installation_id}/access_tokens",
        github_api_base()
    );
    let client = http_client();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "task-backend")
        .send()
        .await
        .context("github installation token request")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "github installation token failed: {status} {body}"
        ));
    }
    let body: InstallationTokenResponse = response
        .json()
        .await
        .context("parse installation token response")?;
    let expires_at = chrono::DateTime::parse_from_rfc3339(&body.expires_at)
        .context("parse token expires_at")?;
    Ok(InstallationAccessToken {
        token: body.token,
        expires_at,
    })
}

pub async fn fetch_installation(
    settings: &GithubAppSettings,
    installation_id: i64,
) -> Result<InstallationInfo, anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!("{}/app/installations/{installation_id}", github_api_base());
    let client = http_client();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "task-backend")
        .send()
        .await
        .context("github get installation")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("github get installation failed: {status} {body}"));
    }
    let body: InstallationResponse = response
        .json()
        .await
        .context("parse installation response")?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&body.created_at)
        .context("parse installation created_at")?;
    Ok(InstallationInfo {
        id: body.id,
        account_login: body.account.login,
        created_at,
    })
}

/// OAuth コールバックで installation_id を GitHub API 経由で検証する。
pub async fn verify_installation_for_callback(
    settings: &GithubAppSettings,
    installation_id: i64,
    expected_installation_id: Option<i64>,
) -> Result<InstallationInfo, anyhow::Error> {
    let info = fetch_installation(settings, installation_id).await?;

    if info.id != installation_id {
        return Err(anyhow!(
            "installation id mismatch: api={} query={installation_id}",
            info.id
        ));
    }

    if let Some(expected) = expected_installation_id {
        if expected != installation_id {
            return Err(anyhow!(
                "installation id does not match oauth state binding"
            ));
        }
    } else {
        let max_age = chrono::Duration::seconds(TTL_SECS as i64);
        let cutoff = chrono::Utc::now().fixed_offset() - max_age;
        if info.created_at < cutoff {
            return Err(anyhow!(
                "installation is too old to bind on first connect (created_at={})",
                info.created_at
            ));
        }
    }

    Ok(info)
}

/// インストール先リポジトリを選定する（テスト可能な純関数）。
pub fn select_primary_repository<'a>(
    repositories: &'a [InstallationRepository],
    preferred_owner: &str,
) -> Option<&'a InstallationRepository> {
    repositories
        .iter()
        .find(|r| r.owner.login == preferred_owner)
        .or_else(|| repositories.first())
}

pub async fn fetch_primary_repository(
    installation_access_token: &str,
    preferred_owner: &str,
) -> Result<(String, String), anyhow::Error> {
    let client = http_client();
    let response = client
        .get(format!("{}/installation/repositories", github_api_base()))
        .header(
            "Authorization",
            format!("Bearer {installation_access_token}"),
        )
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "task-backend")
        .send()
        .await
        .context("github list installation repositories")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "github list repositories failed: {status} {body}"
        ));
    }
    let body: InstallationRepositoriesResponse = response
        .json()
        .await
        .context("parse installation repositories")?;
    let repo = select_primary_repository(&body.repositories, preferred_owner).ok_or_else(|| {
        anyhow!("no repositories accessible for installation")
    })?;
    let (owner, name) = repo
        .full_name
        .split_once('/')
        .ok_or_else(|| anyhow!("invalid repository full_name: {}", repo.full_name))?;
    Ok((owner.to_string(), name.to_string()))
}

pub async fn delete_app_installation(
    settings: &GithubAppSettings,
    installation_id: i64,
) -> Result<(), anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!("{}/app/installations/{installation_id}", github_api_base());
    let client = http_client();
    let response = client
        .delete(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "task-backend")
        .send()
        .await
        .context("github delete installation")?;
    if !response.status().is_success()
        && response.status() != reqwest::StatusCode::NOT_FOUND
        && response.status() != reqwest::StatusCode::GONE
    {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "github delete installation failed: {status} {body}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(full_name: &str, owner: &str) -> InstallationRepository {
        InstallationRepository {
            full_name: full_name.to_string(),
            owner: RepositoryOwner {
                login: owner.to_string(),
            },
        }
    }

    #[test]
    fn select_primary_repository_prefers_installation_account_owner() {
        let repos = vec![
            repo("other-org/app", "other-org"),
            repo("myorg/backend", "myorg"),
            repo("myorg/frontend", "myorg"),
        ];
        let chosen = select_primary_repository(&repos, "myorg").unwrap();
        assert_eq!(chosen.full_name, "myorg/backend");
    }

    #[test]
    fn select_primary_repository_falls_back_to_first() {
        let repos = vec![repo("other-org/app", "other-org")];
        let chosen = select_primary_repository(&repos, "myorg").unwrap();
        assert_eq!(chosen.full_name, "other-org/app");
    }
}

//! GitHub App JWT 発行と Installation Access Token 取得。

use anyhow::{anyhow, Context};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

use crate::settings::GithubAppSettings;

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
struct InstallationAccountResponse {
    account: RepositoryOwner,
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
    let client = reqwest::Client::new();
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

pub async fn fetch_installation_account_login(
    settings: &GithubAppSettings,
    installation_id: i64,
) -> Result<String, anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!("{}/app/installations/{installation_id}", github_api_base());
    let client = reqwest::Client::new();
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
    let body: InstallationAccountResponse = response
        .json()
        .await
        .context("parse installation response")?;
    Ok(body.account.login)
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
    let client = reqwest::Client::new();
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
    let client = reqwest::Client::new();
    let response = client
        .delete(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "task-backend")
        .send()
        .await
        .context("github delete installation")?;
    if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "github delete installation failed: {status} {body}"
        ));
    }
    Ok(())
}

/// Wave 1 で Installation Access Token の自動更新に使用予定。
#[allow(dead_code)]
pub async fn refresh_token_if_needed(
    settings: &GithubAppSettings,
    installation_id: i64,
    token_expires_at: chrono::DateTime<chrono::FixedOffset>,
    access_token_enc: &str,
) -> Result<(String, chrono::DateTime<chrono::FixedOffset>), anyhow::Error> {
    let refresh_threshold =
        chrono::Utc::now().fixed_offset() + chrono::Duration::minutes(5);
    if token_expires_at > refresh_threshold {
        return Ok((
            crate::utils::github_token_crypto::decrypt_token(
                &settings.github_token_encryption_key,
                access_token_enc,
            )?,
            token_expires_at,
        ));
    }
    let fresh = fetch_installation_access_token(settings, installation_id).await?;
    Ok((fresh.token, fresh.expires_at))
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

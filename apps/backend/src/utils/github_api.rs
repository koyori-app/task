//! GitHub App JWT 発行と Installation Access Token 取得。

use anyhow::{anyhow, Context};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

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
pub struct InstallationRepository {
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<InstallationRepository>,
}

pub struct InstallationAccessToken {
    pub token: String,
    pub expires_at: chrono::DateTime<chrono::FixedOffset>,
}

pub fn create_app_jwt(settings: &Settings) -> Result<String, anyhow::Error> {
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
    settings: &Settings,
    installation_id: i64,
) -> Result<InstallationAccessToken, anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!(
        "https://api.github.com/app/installations/{installation_id}/access_tokens"
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

pub async fn fetch_primary_repository(
    _settings: &Settings,
    installation_access_token: &str,
) -> Result<(String, String), anyhow::Error> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/installation/repositories")
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
    let repo = body
        .repositories
        .first()
        .ok_or_else(|| anyhow!("no repositories accessible for installation"))?;
    let (owner, name) = repo
        .full_name
        .split_once('/')
        .ok_or_else(|| anyhow!("invalid repository full_name: {}", repo.full_name))?;
    Ok((owner.to_string(), name.to_string()))
}

pub async fn refresh_token_if_needed(
    settings: &Settings,
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

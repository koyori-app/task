//! GitHub App JWT 発行と Installation Access Token 取得。

use anyhow::{anyhow, Context};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::settings::GithubAppSettings;
use crate::utils::github_oauth_state::TTL_SECS;

fn github_api_base() -> &'static str {
    // テスト用オーバーライド以外は定数。OnceLock でキャッシュして毎回 env lookup しない。
    use std::sync::OnceLock;
    static BASE: OnceLock<String> = OnceLock::new();
    BASE.get_or_init(|| {
        std::env::var("GITHUB_API_BASE_URL")
            .unwrap_or_else(|_| "https://api.github.com".to_string())
            .trim_end_matches('/')
            .to_string()
    })
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
        // iat を 60 秒前に設定してサーバ時刻のわずかなズレによる「issued in future」拒否を防ぐ
        iat: (now - Duration::seconds(60)).timestamp(),
        exp: (now + Duration::minutes(9)).timestamp(),
    };
    let key = EncodingKey::from_rsa_pem(settings.github_app_private_key.as_bytes())
        .context("parse github app private key PEM")?;
    encode(&Header::new(Algorithm::RS256), &claims, &key).context("encode github app jwt")
}

pub async fn fetch_installation_access_token(
    client: &Client,
    settings: &GithubAppSettings,
    installation_id: i64,
) -> Result<InstallationAccessToken, anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!(
        "{}/app/installations/{installation_id}/access_tokens",
        github_api_base()
    );
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
        return Err(anyhow!("github installation token failed: {status} {body}"));
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
    client: &Client,
    settings: &GithubAppSettings,
    installation_id: i64,
) -> Result<InstallationInfo, anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!("{}/app/installations/{installation_id}", github_api_base());
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
    client: &Client,
    settings: &GithubAppSettings,
    installation_id: i64,
    expected_installation_id: Option<i64>,
) -> Result<InstallationInfo, anyhow::Error> {
    let info = fetch_installation(client, settings, installation_id).await?;

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
        // 新規インストール: state の TTL 内に作成されたものだけ受け付ける
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
/// アクセス可能なリポジトリが 1 件のときのみ自動選択する。
/// 複数ある場合はユーザーの明示的選択が必要なため `None` を返す。
pub fn select_primary_repository<'a>(
    repositories: &'a [InstallationRepository],
    _preferred_owner: &str,
) -> Option<&'a InstallationRepository> {
    if repositories.len() == 1 {
        repositories.first()
    } else {
        None
    }
}

pub async fn fetch_primary_repository(
    client: &Client,
    installation_access_token: &str,
    preferred_owner: &str,
) -> Result<(String, String), anyhow::Error> {
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
        return Err(anyhow!("github list repositories failed: {status} {body}"));
    }
    let body: InstallationRepositoriesResponse = response
        .json()
        .await
        .context("parse installation repositories")?;
    let repo =
        select_primary_repository(&body.repositories, preferred_owner).ok_or_else(|| {
            anyhow!(
                "installation has access to {} repositories; select one explicitly",
                body.repositories.len()
            )
        })?;
    let (owner, name) = repo
        .full_name
        .split_once('/')
        .ok_or_else(|| anyhow!("invalid repository full_name: {}", repo.full_name))?;
    Ok((owner.to_string(), name.to_string()))
}

pub async fn delete_app_installation(
    client: &Client,
    settings: &GithubAppSettings,
    installation_id: i64,
) -> Result<(), anyhow::Error> {
    let jwt = create_app_jwt(settings)?;
    let url = format!("{}/app/installations/{installation_id}", github_api_base());
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
    fn select_primary_repository_returns_none_for_multiple_repos() {
        let repos = vec![
            repo("other-org/app", "other-org"),
            repo("myorg/backend", "myorg"),
            repo("myorg/frontend", "myorg"),
        ];
        assert!(select_primary_repository(&repos, "myorg").is_none());
    }

    #[test]
    fn select_primary_repository_auto_selects_single_repo() {
        let repos = vec![repo("other-org/app", "other-org")];
        let chosen = select_primary_repository(&repos, "myorg").unwrap();
        assert_eq!(chosen.full_name, "other-org/app");
    }
}

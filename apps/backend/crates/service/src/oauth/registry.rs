//! provider slug から [`OAuthProvider`] 実装を解決する。
//!
//! GitHub / GitLab は専用クレートの実装を使う。Google と汎用 OIDC は
//! `auth_core` の OIDC 機構（discovery + 標準 userinfo）に薄くかぶせるだけなので
//! ここに置く。GitHub App のユーザー認可（`github_app`）も、アプリ固有の用途
//! （PR を本人名義で Approve する）なのでここに置く。

use anyhow::Error;
use async_trait::async_trait;
use auth_core::client::fetch_oidc_user;
use auth_core::provider::{
    OAuthProvider, ProviderConfig, ProviderEndpoints, ProviderUserInfo, fetch_oidc_discovery,
};
use auth_core_github::GithubProvider;
use auth_core_gitlab::{GitlabProvider, GitlabSelfHostedProvider};
use reqwest::Client;
use serde::Deserialize;

use super::settings::{GITHUB_APP_PROVIDER, OAuthSettings};
use crate::github::client::{api_base, oauth_base};

/// GitHub API に送る User-Agent。
const GITHUB_USER_AGENT: &str = "task-oauth-backend";

/// Google の OAuth ログインプロバイダー（エンドポイントは固定、userinfo は OIDC 標準）。
struct GoogleProvider;

#[async_trait]
impl OAuthProvider for GoogleProvider {
    fn slug(&self) -> &str {
        "google"
    }

    async fn endpoints(&self, _http: &Client) -> Result<ProviderEndpoints, Error> {
        Ok(ProviderEndpoints {
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".to_string(),
            scopes: vec!["openid", "email", "profile"],
            use_oidc_id_token: true,
        })
    }

    async fn fetch_user_info(
        &self,
        http: &Client,
        endpoints: &ProviderEndpoints,
        access_token: &str,
    ) -> Result<ProviderUserInfo, Error> {
        fetch_oidc_user(http, &endpoints.userinfo_url, access_token).await
    }
}

/// GitHub App のユーザー認可。利用者本人の名義で PR を Approve するトークンを取る。
///
/// トークンの権限は「App の権限 ∩ 本人の権限」で、scope は無い。ログイン用の
/// [`GithubProvider`]（OAuth App）と違い `/user/emails` は読まない——App の
/// 「Email addresses」権限が要り、連携専用なのでメールアドレスも要らない。
pub struct GithubAppProvider;

#[derive(Deserialize)]
struct GithubAppUser {
    id: i64,
    login: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

#[async_trait]
impl OAuthProvider for GithubAppProvider {
    fn slug(&self) -> &str {
        GITHUB_APP_PROVIDER
    }

    async fn endpoints(&self, _http: &Client) -> Result<ProviderEndpoints, Error> {
        let oauth = oauth_base();
        Ok(ProviderEndpoints {
            authorize_url: format!("{oauth}/login/oauth/authorize"),
            token_url: format!("{oauth}/login/oauth/access_token"),
            userinfo_url: format!("{}/user", api_base()),
            scopes: vec![],
            use_oidc_id_token: false,
        })
    }

    async fn fetch_user_info(
        &self,
        http: &Client,
        endpoints: &ProviderEndpoints,
        access_token: &str,
    ) -> Result<ProviderUserInfo, Error> {
        let user: GithubAppUser = http
            .get(&endpoints.userinfo_url)
            .bearer_auth(access_token)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", GITHUB_USER_AGENT)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(ProviderUserInfo {
            provider_user_id: user.id.to_string(),
            email: None,
            email_verified: None,
            username: user.login,
            avatar_url: user.avatar_url,
        })
    }
}

/// 汎用 OIDC プロバイダー。エンドポイントは discovery で解決する。
struct OidcProvider {
    issuer_url: String,
}

#[async_trait]
impl OAuthProvider for OidcProvider {
    fn slug(&self) -> &str {
        "oidc"
    }

    async fn endpoints(&self, http: &Client) -> Result<ProviderEndpoints, Error> {
        // issuer は運用者が設定する値だが、内部ネットワークを走査させないため検証する。
        auth_core::url_guard::validate_instance_url(&self.issuer_url)?;
        fetch_oidc_discovery(http, &self.issuer_url).await
    }

    async fn fetch_user_info(
        &self,
        http: &Client,
        endpoints: &ProviderEndpoints,
        access_token: &str,
    ) -> Result<ProviderUserInfo, Error> {
        fetch_oidc_user(http, &endpoints.userinfo_url, access_token).await
    }
}

/// provider slug に対応する実装を返す。
///
/// `instance_url` は `gitlab_selfhosted` のみで必要（他のプロバイダーでは無視される）。
/// `gitlab_selfhosted` は、運用者の許可の一覧（[`OAuthSettings::gitlab_selfhosted_instance`]）に
/// 在る instance だけを解決する。
pub fn resolve_provider(
    provider_slug: &str,
    settings: &OAuthSettings,
    instance_url: Option<&str>,
) -> Result<Box<dyn OAuthProvider>, Error> {
    match provider_slug {
        "github" => Ok(Box::new(GithubProvider::new(GITHUB_USER_AGENT))),
        "gitlab" => Ok(Box::new(GitlabProvider)),
        "gitlab_selfhosted" => {
            let base = instance_url
                .ok_or_else(|| anyhow::anyhow!("instance_url is required for gitlab_selfhosted"))?;
            let instance = settings.gitlab_selfhosted_instance(base).ok_or_else(|| {
                anyhow::anyhow!("gitlab_selfhosted instance is not in the allowlist")
            })?;
            Ok(Box::new(GitlabSelfHostedProvider::new(&instance.url)?))
        }
        "google" => Ok(Box::new(GoogleProvider)),
        "oidc" => {
            let issuer_url = settings
                .oidc
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("oidc provider not configured"))?
                .issuer_url
                .clone();
            Ok(Box::new(OidcProvider { issuer_url }))
        }
        GITHUB_APP_PROVIDER => Ok(Box::new(GithubAppProvider)),
        other => anyhow::bail!("unsupported oauth provider: {other}"),
    }
}

/// provider の client の資格情報。`gitlab_selfhosted` は許可の一覧に在る instance のものを返す。
pub fn get_credentials(
    provider_slug: &str,
    settings: &OAuthSettings,
    instance_url: Option<&str>,
) -> Result<ProviderConfig, Error> {
    match provider_slug {
        "github" => settings
            .github
            .clone()
            .ok_or_else(|| anyhow::anyhow!("github provider not configured")),
        "gitlab" => settings
            .gitlab
            .clone()
            .ok_or_else(|| anyhow::anyhow!("gitlab provider not configured")),
        "gitlab_selfhosted" => instance_url
            .and_then(|url| settings.gitlab_selfhosted_instance(url))
            .map(|instance| instance.credentials.clone())
            .ok_or_else(|| anyhow::anyhow!("gitlab_selfhosted instance is not in the allowlist")),
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
        GITHUB_APP_PROVIDER => settings
            .github_app
            .clone()
            .ok_or_else(|| anyhow::anyhow!("github_app provider not configured")),
        other => anyhow::bail!("unsupported oauth provider: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> OAuthSettings {
        OAuthSettings {
            app_base_url: "http://localhost:3400".into(),
            encryption_key: "k".repeat(32),
            default_redirect_path: "/dashboard".into(),
            github: None,
            gitlab: None,
            gitlab_selfhosted_instances: Vec::new(),
            google: None,
            oidc: None,
            github_app: None,
        }
    }

    #[test]
    fn resolves_known_providers() {
        let s = settings();
        assert_eq!(
            resolve_provider("github", &s, None).unwrap().slug(),
            "github"
        );
        assert_eq!(
            resolve_provider("gitlab", &s, None).unwrap().slug(),
            "gitlab"
        );
        assert_eq!(
            resolve_provider("google", &s, None).unwrap().slug(),
            "google"
        );
    }

    #[test]
    fn rejects_unknown_provider() {
        assert!(resolve_provider("bitbucket", &settings(), None).is_err());
    }

    #[test]
    fn self_hosted_requires_instance_url() {
        assert!(resolve_provider("gitlab_selfhosted", &settings(), None).is_err());
    }

    #[test]
    fn self_hosted_rejects_private_instance_url() {
        // SSRF: 内部ネットワークへ向いた instance_url は解決段階で弾く。
        assert!(
            resolve_provider("gitlab_selfhosted", &settings(), Some("https://10.0.0.5")).is_err()
        );
    }

    fn settings_with_instance(url: &str, client_id: &str) -> OAuthSettings {
        OAuthSettings {
            gitlab_selfhosted_instances: vec![super::super::settings::GitlabSelfHostedInstance {
                url: url.to_string(),
                credentials: ProviderConfig {
                    client_id: client_id.to_string(),
                    client_secret: format!("{client_id}-secret"),
                },
            }],
            ..settings()
        }
    }

    #[test]
    fn self_hosted_resolves_only_allowlisted_instances() {
        let s = settings_with_instance("http://localhost:8080", "listed");
        assert!(resolve_provider("gitlab_selfhosted", &s, Some("http://LOCALHOST:8080/")).is_ok());
        assert!(resolve_provider("gitlab_selfhosted", &s, Some("http://localhost:8081")).is_err());
        // 一覧が空なら、どの instance も解決しない
        assert!(
            resolve_provider(
                "gitlab_selfhosted",
                &settings(),
                Some("http://localhost:8080")
            )
            .is_err()
        );
    }

    #[test]
    fn self_hosted_credentials_follow_the_instance() {
        let s = settings_with_instance("http://localhost:8080", "listed");
        assert_eq!(
            get_credentials("gitlab_selfhosted", &s, Some("http://localhost:8080"))
                .unwrap()
                .client_id,
            "listed"
        );
        assert!(get_credentials("gitlab_selfhosted", &s, Some("http://localhost:8081")).is_err());
        assert!(get_credentials("gitlab_selfhosted", &s, None).is_err());
    }

    #[test]
    fn oidc_requires_configuration() {
        assert!(resolve_provider("oidc", &settings(), None).is_err());
    }
}

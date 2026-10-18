//! OAuth 設定（環境変数から読み込み）。

use std::collections::HashMap;

use auth_core::provider::ProviderConfig;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct OAuthSettings {
    /// OAuth コールバック URL のベース（例: `http://localhost:3400`）
    pub app_base_url: String,
    /// トークン暗号化の鍵材料。実際の鍵は HKDF で導出される
    pub encryption_key: String,
    /// ログイン後のデフォルトリダイレクト先（フロント相対パス）
    pub default_redirect_path: String,
    pub github: Option<ProviderConfig>,
    pub gitlab: Option<ProviderConfig>,
    /// 運用者が許した GitLab self-hosted の instance。空なら `gitlab_selfhosted` は無効
    pub gitlab_selfhosted_instances: Vec<GitlabSelfHostedInstance>,
    pub google: Option<ProviderConfig>,
    pub oidc: Option<OidcConfig>,
    /// GitHub App のユーザー認可（PR を利用者本人の名義で Approve するための連携）。
    /// ログインには使わない（[`OAuthSettings::is_sign_in_provider`]）
    pub github_app: Option<ProviderConfig>,
}

/// 運用者が許した GitLab self-hosted の instance と、その instance に登録した OAuth app の資格情報。
#[derive(Clone, Debug)]
pub struct GitlabSelfHostedInstance {
    /// 正規化した instance の URL（[`canonical_instance_url`]）
    pub url: String,
    pub credentials: ProviderConfig,
}

/// `OAUTH_GITLAB_SELFHOSTED_INSTANCE_CREDENTIALS` の一項。
#[derive(Deserialize)]
struct InstanceCredentials {
    client_id: String,
    client_secret: String,
}

/// GitHub App のユーザー認可の provider slug。DB の `provider` も同じ値。
pub const GITHUB_APP_PROVIDER: &str = "github_app";

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
        let gitlab_selfhosted_shared = pair_config(
            env_var("OAUTH_GITLAB_SELFHOSTED_CLIENT_ID"),
            env_var("OAUTH_GITLAB_SELFHOSTED_CLIENT_SECRET"),
        );
        let has_shared_selfhosted = gitlab_selfhosted_shared.is_some();
        let gitlab_selfhosted_instances = parse_gitlab_selfhosted_instances(
            env_var("OAUTH_GITLAB_SELFHOSTED_INSTANCE_URLS").as_deref(),
            gitlab_selfhosted_shared,
            env_var("OAUTH_GITLAB_SELFHOSTED_INSTANCE_CREDENTIALS").as_deref(),
        )?;
        if gitlab_selfhosted_instances.is_empty() && has_shared_selfhosted {
            tracing::warn!(
                "OAUTH_GITLAB_SELFHOSTED_CLIENT_ID is set but OAUTH_GITLAB_SELFHOSTED_INSTANCE_URLS is empty; gitlab_selfhosted stays disabled"
            );
        }
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
            || !gitlab_selfhosted_instances.is_empty()
            || google.is_some()
            || oidc.is_some();

        let encryption_key =
            parse_encryption_key(env_var("OAUTH_ENCRYPTION_KEY").as_deref(), has_providers)?;

        // GitHub App の資格情報は App 連携の設定（GITHUB_APP_*）と共有する。暗号鍵が無ければ
        // 有効にしない——ここで鍵を必須にすると、ログイン用 OAuth を使っていない既存の環境が
        // 起動しなくなる
        let github_app = pair_config(
            env_var("GITHUB_APP_CLIENT_ID"),
            env_var("GITHUB_APP_CLIENT_SECRET"),
        )
        .filter(|_| !encryption_key.is_empty());

        let default_redirect_path =
            env_var("OAUTH_DEFAULT_REDIRECT_PATH").unwrap_or_else(|| "/dashboard".to_string());

        Ok(Self {
            app_base_url: app_base_url.trim_end_matches('/').to_string(),
            encryption_key,
            default_redirect_path,
            github,
            gitlab,
            gitlab_selfhosted_instances,
            google,
            oidc,
            github_app,
        })
    }

    /// 許可の一覧に在る GitLab self-hosted の instance。`raw` は正規化してから突き合わせる。
    pub fn gitlab_selfhosted_instance(&self, raw: &str) -> Option<&GitlabSelfHostedInstance> {
        let url = canonical_instance_url(raw).ok()?;
        self.gitlab_selfhosted_instances
            .iter()
            .find(|instance| instance.url == url)
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
            "gitlab_selfhosted" => !self.gitlab_selfhosted_instances.is_empty(),
            "google" => self.google.is_some(),
            "oidc" => self.oidc.is_some(),
            GITHUB_APP_PROVIDER => self.github_app.is_some(),
            _ => false,
        }
    }

    /// サインインに使えるプロバイダーか。`github_app` は既存の利用者への連携専用で、
    /// これで新しくログインしたり利用者を作ったりはしない。
    pub fn is_sign_in_provider(provider_slug: &str) -> bool {
        provider_slug != GITHUB_APP_PROVIDER
    }

    pub fn has_any_provider(&self) -> bool {
        self.github.is_some()
            || self.gitlab.is_some()
            || !self.gitlab_selfhosted_instances.is_empty()
            || self.google.is_some()
            || self.oidc.is_some()
    }

    /// DB に保存する provider キー（汎用 OIDC は `oidc:{issuer}`）。
    pub fn db_provider_key(&self, provider_slug: &str) -> Option<String> {
        match provider_slug {
            "github" | "gitlab" | "gitlab_selfhosted" | "google" | GITHUB_APP_PROVIDER => {
                Some(provider_slug.to_string())
            }
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

/// instance の URL を比べられる形に揃える。
///
/// scheme と host を小文字にし、既定の port（https の 443・http の 80）と末尾の `/` を落とす。
/// 資格情報・query・fragment を含む URL は受けない。許可の一覧と、利用者が渡す URL の両方を
/// これに通してから突き合わせる。
pub fn canonical_instance_url(raw: &str) -> Result<String, anyhow::Error> {
    let parsed = url::Url::parse(raw.trim())?;
    let scheme = parsed.scheme();
    if scheme != "https" && scheme != "http" {
        anyhow::bail!("instance URL must use https or http");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("instance URL must not include credentials");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        anyhow::bail!("instance URL must not include a query or fragment");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("instance URL must include a host"))?
        .to_ascii_lowercase();
    // url crate は scheme の既定の port を None にする
    let port = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
    let path = parsed.path().trim_end_matches('/');
    Ok(format!("{scheme}://{host}{port}{path}"))
}

/// GitLab self-hosted の許可の一覧と、instance ごとの資格情報を組む。
///
/// - `urls`: `OAUTH_GITLAB_SELFHOSTED_INSTANCE_URLS`（カンマ区切り）。空なら一覧も空で、
///   `gitlab_selfhosted` は無効になる
/// - `shared`: `OAUTH_GITLAB_SELFHOSTED_CLIENT_ID` / `_SECRET`。一覧の全ての instance の既定
/// - `per_instance`: `OAUTH_GITLAB_SELFHOSTED_INSTANCE_CREDENTIALS`（JSON の
///   `{"<url>": {"client_id": "…", "client_secret": "…"}}`）。その instance だけ既定に代える
///
/// 資格情報の無い instance や、一覧に無い instance の資格情報は設定の誤りとして起動を止める。
pub fn parse_gitlab_selfhosted_instances(
    urls: Option<&str>,
    shared: Option<ProviderConfig>,
    per_instance: Option<&str>,
) -> Result<Vec<GitlabSelfHostedInstance>, anyhow::Error> {
    let mut overrides: HashMap<String, ProviderConfig> = HashMap::new();
    if let Some(raw) = per_instance {
        let parsed: HashMap<String, InstanceCredentials> =
            serde_json::from_str(raw).map_err(|e| {
                anyhow::anyhow!(
                    "OAUTH_GITLAB_SELFHOSTED_INSTANCE_CREDENTIALS must be a JSON object of instance URL to {{\"client_id\", \"client_secret\"}}: {e}"
                )
            })?;
        for (url, credentials) in parsed {
            overrides.insert(
                canonical_instance_url(&url)?,
                ProviderConfig {
                    client_id: credentials.client_id,
                    client_secret: credentials.client_secret,
                },
            );
        }
    }

    let mut instances: Vec<GitlabSelfHostedInstance> = Vec::new();
    for raw in urls
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let url = canonical_instance_url(raw)?;
        if instances.iter().any(|instance| instance.url == url) {
            continue;
        }
        let credentials = overrides
            .remove(&url)
            .or_else(|| shared.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no OAuth client credentials for GitLab self-hosted instance {url}: set OAUTH_GITLAB_SELFHOSTED_CLIENT_ID/SECRET or OAUTH_GITLAB_SELFHOSTED_INSTANCE_CREDENTIALS"
                )
            })?;
        instances.push(GitlabSelfHostedInstance { url, credentials });
    }

    if let Some(url) = overrides.keys().next() {
        anyhow::bail!(
            "OAUTH_GITLAB_SELFHOSTED_INSTANCE_CREDENTIALS lists {url}, which is not in OAUTH_GITLAB_SELFHOSTED_INSTANCE_URLS"
        );
    }
    Ok(instances)
}

/// 鍵材料をそのまま保持する（鍵の導出は `auth_core::crypto` 側の HKDF が行う）。
fn parse_encryption_key(raw: Option<&str>, require: bool) -> Result<String, anyhow::Error> {
    let Some(key_str) = raw.filter(|s| !s.is_empty()) else {
        if require {
            anyhow::bail!(
                "OAUTH_ENCRYPTION_KEY is required when at least one OAuth provider is configured"
            );
        }
        return Ok(String::new());
    };

    if key_str.len() < 32 {
        anyhow::bail!("OAUTH_ENCRYPTION_KEY must be at least 32 characters");
    }

    Ok(key_str.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_key_optional_without_providers() {
        assert_eq!(parse_encryption_key(None, false).unwrap(), "");
    }

    #[test]
    fn encryption_key_required_with_providers() {
        assert!(parse_encryption_key(None, true).is_err());
    }

    #[test]
    fn encryption_key_rejects_short_value() {
        assert!(parse_encryption_key(Some(&"a".repeat(31)), true).is_err());
    }

    /// 連携一覧が返す識別子。OIDC だけ開始用 slug と形が違うので、画面はこちらで突き合わせる。
    #[test]
    fn db_provider_key_carries_oidc_issuer() {
        let mut settings = OAuthSettings {
            app_base_url: "http://localhost:3400".to_string(),
            encryption_key: "k".repeat(32),
            default_redirect_path: "/dashboard".to_string(),
            github: None,
            gitlab: None,
            gitlab_selfhosted_instances: Vec::new(),
            google: None,
            github_app: None,
            oidc: Some(OidcConfig {
                issuer_url: "https://idp.example.com".to_string(),
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
            }),
        };
        assert_eq!(
            settings.db_provider_key("oidc").as_deref(),
            Some("oidc:https://idp.example.com")
        );
        assert_eq!(
            settings.db_provider_key("github").as_deref(),
            Some("github")
        );

        settings.oidc = None;
        assert_eq!(settings.db_provider_key("oidc"), None);
    }

    fn config(id: &str) -> ProviderConfig {
        ProviderConfig {
            client_id: id.to_string(),
            client_secret: format!("{id}-secret"),
        }
    }

    #[test]
    fn canonical_instance_url_folds_case_default_port_and_trailing_slash() {
        for raw in [
            "https://gitlab.example.com",
            "https://GitLab.Example.COM/",
            "HTTPS://gitlab.example.com:443",
            " https://gitlab.example.com// ",
        ] {
            assert_eq!(
                canonical_instance_url(raw).unwrap(),
                "https://gitlab.example.com",
                "{raw}"
            );
        }
        assert_eq!(
            canonical_instance_url("http://localhost:8080/gitlab/").unwrap(),
            "http://localhost:8080/gitlab"
        );
        assert_eq!(
            canonical_instance_url("https://gitlab.example.com:8443").unwrap(),
            "https://gitlab.example.com:8443"
        );
    }

    #[test]
    fn canonical_instance_url_rejects_unusual_forms() {
        for raw in [
            "ftp://gitlab.example.com",
            "https://user:pass@gitlab.example.com",
            "https://gitlab.example.com/?a=1",
            "https://gitlab.example.com/#x",
            "gitlab.example.com",
        ] {
            assert!(canonical_instance_url(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn selfhosted_instances_are_empty_without_a_list() {
        assert!(
            parse_gitlab_selfhosted_instances(None, Some(config("shared")), None)
                .unwrap()
                .is_empty()
        );
        assert!(
            parse_gitlab_selfhosted_instances(Some(" , "), Some(config("shared")), None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn selfhosted_instances_use_shared_credentials_unless_overridden() {
        let instances = parse_gitlab_selfhosted_instances(
            Some("https://a.example.com, https://B.example.com/ ,https://a.example.com/"),
            Some(config("shared")),
            Some(r#"{"https://b.example.com": {"client_id": "b", "client_secret": "b-secret"}}"#),
        )
        .unwrap();
        let pairs: Vec<(&str, &str)> = instances
            .iter()
            .map(|i| (i.url.as_str(), i.credentials.client_id.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("https://a.example.com", "shared"),
                ("https://b.example.com", "b")
            ]
        );
    }

    #[test]
    fn selfhosted_instances_reject_inconsistent_configuration() {
        // 資格情報の無い instance
        assert!(
            parse_gitlab_selfhosted_instances(Some("https://a.example.com"), None, None).is_err()
        );
        // 一覧に無い instance の資格情報
        assert!(
            parse_gitlab_selfhosted_instances(
                Some("https://a.example.com"),
                Some(config("shared")),
                Some(r#"{"https://c.example.com": {"client_id": "c", "client_secret": "s"}}"#),
            )
            .is_err()
        );
        // JSON の形の誤り
        assert!(
            parse_gitlab_selfhosted_instances(
                Some("https://a.example.com"),
                Some(config("shared")),
                Some("not json"),
            )
            .is_err()
        );
    }

    #[test]
    fn selfhosted_lookup_matches_only_listed_instances() {
        let settings = OAuthSettings {
            app_base_url: "http://localhost:3400".to_string(),
            encryption_key: "k".repeat(32),
            default_redirect_path: "/dashboard".to_string(),
            github: None,
            gitlab: None,
            gitlab_selfhosted_instances: parse_gitlab_selfhosted_instances(
                Some("https://gitlab.example.com"),
                Some(config("shared")),
                None,
            )
            .unwrap(),
            google: None,
            oidc: None,
            github_app: None,
        };
        assert!(settings.is_provider_configured("gitlab_selfhosted"));
        assert!(
            settings
                .gitlab_selfhosted_instance("https://GITLAB.example.com/")
                .is_some()
        );
        assert!(
            settings
                .gitlab_selfhosted_instance("https://gitlab.example.net")
                .is_none()
        );
        let disabled = OAuthSettings {
            gitlab_selfhosted_instances: Vec::new(),
            ..settings
        };
        assert!(!disabled.is_provider_configured("gitlab_selfhosted"));
    }

    #[test]
    fn encryption_key_keeps_full_material() {
        // 先頭 32 バイトへの切り詰めをしない（鍵材料全体が HKDF に入る）。
        let long = "a".repeat(64);
        assert_eq!(parse_encryption_key(Some(&long), true).unwrap(), long);
    }
}

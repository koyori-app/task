//! MCP の入口のための OAuth の認可サーバー（認可コード + PKCE、CIMD、公開クライアントだけ）。
//! 仕様は docs/dev/features/mcp.md の「OAuth での接続」。
//!
//! ログイン用の OAuth クライアント（[`crate::oauth`]）とは別物である。

use common::settings::Settings;
use entity::scopes::Scope;
use url::Url;

pub mod client;
pub mod grant;
mod vetted;

/// アクセストークンの接頭辞。Bearer の値がこれで始まれば PAT ではなく OAuth のトークンとして引く。
pub const ACCESS_TOKEN_PREFIX: &str = "mcpat_";
/// リフレッシュトークンの接頭辞。
pub const REFRESH_TOKEN_PREFIX: &str = "mcprt_";
/// アクセストークンの寿命。
pub const ACCESS_TOKEN_TTL_SECS: i64 = 3600;
/// リフレッシュトークンと許可の寿命。更新のたびに延びる。
pub const REFRESH_TOKEN_TTL_DAYS: i64 = 90;
/// 認可コードの寿命。
pub const CODE_TTL_SECS: u64 = 300;

/// OAuth で渡せる scope。MCP のツールが要る scope のうち `write:sprint` を除いたもので、これ以外は渡さない。
/// `write:sprint`（スプリントの開始と完了）は戻せない管理の操作なので渡さない
/// （docs/dev/features/mcp.md の決定の記録）。
pub const GRANTABLE_SCOPES: [Scope; 8] = [
    Scope::ReadTenant,
    Scope::ReadProject,
    Scope::ReadTask,
    Scope::WriteTask,
    Scope::ReadMilestone,
    Scope::ReadSprint,
    Scope::ReadReview,
    Scope::WriteReview,
];

/// リフレッシュトークンを求める scope。受けて捨てる（発行はクライアントの宣言で決める）。
pub const OFFLINE_ACCESS: &str = "offline_access";

/// scope を空白区切りの文字列にする（`WWW-Authenticate` と token の応答の形）。
pub fn scope_string(scopes: &[Scope]) -> String {
    scopes
        .iter()
        .map(Scope::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

/// 認可サーバーの issuer（公開の origin、パス無し）。
pub fn issuer(settings: &Settings) -> String {
    settings.public_origin()
}

/// 保護リソース（MCP の入口）の公開の URL。トークンは常にこれ宛てに発行する。
pub fn resource_uri(settings: &Settings) -> String {
    format!("{}/api/mcp", settings.public_origin())
}

/// Protected Resource Metadata の公開の URL（401 の `resource_metadata` に載せる）。
pub fn protected_resource_metadata_url(settings: &Settings) -> String {
    format!(
        "{}/.well-known/oauth-protected-resource/api/mcp",
        settings.public_origin()
    )
}

/// 要求の `resource` がこのサーバーの MCP の入口を指すか。
///
/// scheme と host の大文字小文字、既定ポートの有無、末尾の `/` 1 つの違いは同じとみなす。
/// query・fragment・userinfo の付いた値は受けない。
pub fn resource_matches(settings: &Settings, raw: &str) -> bool {
    let (Ok(expected), Ok(presented)) = (Url::parse(&resource_uri(settings)), Url::parse(raw))
    else {
        return false;
    };
    if presented.query().is_some()
        || presented.fragment().is_some()
        || !presented.username().is_empty()
        || presented.password().is_some()
    {
        return false;
    }
    let path = presented.path();
    let path = path.strip_suffix('/').unwrap_or(path);
    presented.scheme() == expected.scheme()
        && presented.host_str() == expected.host_str()
        && presented.port_or_known_default() == expected.port_or_known_default()
        && path == expected.path()
}

/// 認可の要求の `scope` を読む。refresh の `scope` には使わない。
///
/// 空白区切り。`offline_access` と [`GRANTABLE_SCOPES`] に無い語は捨てる。省略か、
/// `offline_access` だけなら全部を返す。渡せる語を 1 つも含まない要求は空を返す
/// （呼び出し側が拒否する）。重複はまとめ、順は [`GRANTABLE_SCOPES`] の順にする。
pub fn parse_scope_param(raw: Option<&str>) -> Vec<Scope> {
    let words: Vec<&str> = raw
        .unwrap_or_default()
        .split_whitespace()
        .filter(|word| *word != OFFLINE_ACCESS)
        .collect();
    if words.is_empty() {
        return GRANTABLE_SCOPES.to_vec();
    }
    GRANTABLE_SCOPES
        .into_iter()
        .filter(|scope| words.contains(&scope.as_str()))
        .collect()
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// `redirect_uri` が loopback の http か（同意画面の表示を強める条件）。
pub fn redirect_uri_is_loopback(raw: &str) -> bool {
    Url::parse(raw)
        .is_ok_and(|url| url.scheme() == "http" && url.host_str().is_some_and(is_loopback_host))
}

/// 提示された `redirect_uri` が、クライアントの登録のどれかと合うか。
///
/// 完全一致の文字列比較が原則。loopback の http だけ、ポートを比べない（OAuth 2.1 の例外）。
/// 例外は文字列の加工でなく URL を解いて判定する。`http://localhost:1@evil.example/callback` は
/// host が `evil.example` になるので合わない。
pub fn redirect_uri_matches(registered: &[String], presented: &str) -> bool {
    if registered.iter().any(|uri| uri == presented) {
        return true;
    }
    let Ok(presented) = Url::parse(presented) else {
        return false;
    };
    registered.iter().any(|uri| {
        Url::parse(uri).is_ok_and(|registered| loopback_uris_match(&registered, &presented))
    })
}

fn loopback_uris_match(registered: &Url, presented: &Url) -> bool {
    let plain_loopback_http = |url: &Url| {
        url.scheme() == "http"
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.host_str().is_some_and(is_loopback_host)
    };
    plain_loopback_http(registered)
        && plain_loopback_http(presented)
        && registered.host_str() == presented.host_str()
        && registered.path() == presented.path()
        && registered.query() == presented.query()
}

#[cfg(test)]
pub(crate) fn test_settings(allow_origin: &str, allowed_client_ids: &str) -> Settings {
    let mut settings: Settings = serde_json::from_value(serde_json::json!({
        "database_url": "",
        "redis_url": "",
        "smtp_host": "",
        "smtp_port": 0,
        "smtp_username": "",
        "smtp_password": "",
        "smtp_from": "",
        "email_verification_app_url": "http://localhost:3000",
        "personal_token_secret": "a".repeat(32),
        "recovery_code_secret": "c".repeat(32),
        "totp_encryption_key": "b".repeat(32),
    }))
    .expect("test settings");
    settings.allow_origin = allow_origin.to_string();
    settings.oauth_allowed_client_ids = allowed_client_ids.to_string();
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        test_settings("https://app.example.com", "")
    }

    #[test]
    fn grantable_scopes_are_the_ones_the_tools_need() {
        assert_eq!(
            scope_string(&GRANTABLE_SCOPES),
            "read:tenant read:project read:task write:task read:milestone read:sprint read:review write:review"
        );
    }

    #[test]
    fn urls_are_built_from_the_public_origin() {
        let settings = test_settings("https://app.example.com/", "");
        assert_eq!(issuer(&settings), "https://app.example.com");
        assert_eq!(resource_uri(&settings), "https://app.example.com/api/mcp");
        assert_eq!(
            protected_resource_metadata_url(&settings),
            "https://app.example.com/.well-known/oauth-protected-resource/api/mcp"
        );
    }

    #[test]
    fn resource_matches_the_canonical_form_and_its_equivalents() {
        let settings = settings();
        for raw in [
            "https://app.example.com/api/mcp",
            "https://app.example.com/api/mcp/",
            "HTTPS://APP.EXAMPLE.COM/api/mcp",
            "https://app.example.com:443/api/mcp",
        ] {
            assert!(resource_matches(&settings, raw), "{raw}");
        }
    }

    #[test]
    fn resource_rejects_other_targets() {
        let settings = settings();
        for raw in [
            "https://app.example.com",
            "https://app.example.com/",
            "https://app.example.com/api",
            "https://app.example.com/api/mcp/x",
            "https://app.example.com/api/mcp//",
            "https://app.example.com/API/MCP",
            "https://app.example.com/api/mcp#x",
            "https://app.example.com/api/mcp?x=1",
            "https://user@app.example.com/api/mcp",
            "http://app.example.com/api/mcp",
            "https://app.example.com:8443/api/mcp",
            "https://other.example.com/api/mcp",
            "not a url",
            "",
        ] {
            assert!(!resource_matches(&settings, raw), "{raw}");
        }
    }

    #[test]
    fn scope_param_defaults_to_everything() {
        assert_eq!(parse_scope_param(None), GRANTABLE_SCOPES.to_vec());
        assert_eq!(parse_scope_param(Some("")), GRANTABLE_SCOPES.to_vec());
        assert_eq!(
            parse_scope_param(Some("offline_access")),
            GRANTABLE_SCOPES.to_vec()
        );
    }

    #[test]
    fn scope_param_keeps_only_grantable_words_in_a_fixed_order() {
        assert_eq!(
            parse_scope_param(Some("write:task  read:task write:task offline_access")),
            vec![Scope::ReadTask, Scope::WriteTask]
        );
        // 渡せない語と未知の語は捨てる
        assert_eq!(
            parse_scope_param(Some("write:project read:task api bogus")),
            vec![Scope::ReadTask]
        );
    }

    #[test]
    fn scope_param_with_no_grantable_word_is_empty() {
        assert!(parse_scope_param(Some("write:project")).is_empty());
        assert!(parse_scope_param(Some("api offline_access")).is_empty());
        assert!(parse_scope_param(Some("bogus")).is_empty());
    }

    fn registered(uris: &[&str]) -> Vec<String> {
        uris.iter().map(|uri| (*uri).to_string()).collect()
    }

    #[test]
    fn redirect_uri_matches_exactly() {
        let uris = registered(&["https://claude.ai/api/mcp/auth_callback"]);
        assert!(redirect_uri_matches(
            &uris,
            "https://claude.ai/api/mcp/auth_callback"
        ));
        for presented in [
            "https://claude.ai/api/mcp/auth_callbac",
            "https://claude.ai/api/mcp/auth_callback/",
            "https://claude.ai/api/mcp/other",
            "https://claude.ai:8443/api/mcp/auth_callback",
            "https://claude.ai.evil.example/api/mcp/auth_callback",
            "https://claude.ai/api/mcp/auth_callback?x=1",
            "http://claude.ai/api/mcp/auth_callback",
        ] {
            assert!(!redirect_uri_matches(&uris, presented), "{presented}");
        }
    }

    #[test]
    fn loopback_redirect_uri_ignores_only_the_port() {
        let uris = registered(&["http://localhost/callback", "http://127.0.0.1/callback"]);
        for presented in [
            "http://localhost:53124/callback",
            "http://127.0.0.1:53124/callback",
            "http://localhost/callback",
        ] {
            assert!(redirect_uri_matches(&uris, presented), "{presented}");
        }
        for presented in [
            "http://localhost:53124/other",
            "http://localhost:53124/callback/abc",
            "http://localhost:53124/callback?x=1",
            "http://127.0.0.1:53124/callback#x",
            "http://localhost:1@evil.example/callback",
            "http://localhost.evil.example/callback",
            "http://user@localhost:53124/callback",
            "https://localhost:53124/callback",
            "http://[::1]:53124/callback",
        ] {
            assert!(!redirect_uri_matches(&uris, presented), "{presented}");
        }
    }

    #[test]
    fn loopback_hosts_are_not_interchangeable() {
        let only_localhost = registered(&["http://localhost/callback"]);
        assert!(!redirect_uri_matches(
            &only_localhost,
            "http://127.0.0.1:53124/callback"
        ));
        let only_v6 = registered(&["http://[::1]/callback"]);
        assert!(redirect_uri_matches(
            &only_v6,
            "http://[::1]:53124/callback"
        ));
        assert!(!redirect_uri_matches(
            &only_v6,
            "http://localhost:53124/callback"
        ));
    }

    #[test]
    fn https_redirect_uri_compares_the_port() {
        let uris = registered(&["https://client.example/callback"]);
        assert!(!redirect_uri_matches(
            &uris,
            "https://client.example:8443/callback"
        ));
    }

    #[test]
    fn loopback_detection_reads_the_parsed_host() {
        assert!(redirect_uri_is_loopback("http://127.0.0.1:53124/callback"));
        assert!(redirect_uri_is_loopback("http://localhost/callback"));
        assert!(redirect_uri_is_loopback("http://[::1]:1/callback"));
        assert!(!redirect_uri_is_loopback("https://localhost/callback"));
        assert!(!redirect_uri_is_loopback(
            "http://localhost:1@evil.example/callback"
        ));
        assert!(!redirect_uri_is_loopback("https://claude.ai/callback"));
    }
}

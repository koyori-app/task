use std::sync::Arc;

use url::Url;
use webauthn_rs::prelude::*;

use crate::settings::Settings;

/// `email_verification_app_url` から scheme+host+port のみの origin を構築する。
fn webauthn_origin(settings: &Settings) -> Result<Url, anyhow::Error> {
    let parsed = Url::parse(settings.email_verification_app_url.trim())?;
    let origin = format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("webauthn: origin has no host"))?,
        parsed.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    Url::parse(&origin).map_err(|e| anyhow::anyhow!("webauthn origin: {e}"))
}

pub fn build_webauthn(settings: &Settings) -> Result<Arc<Webauthn>, anyhow::Error> {
    let origin = webauthn_origin(settings)?;
    let rp_id = resolve_rp_id(settings.webauthn_rp_id.as_deref(), &origin)
        .ok_or_else(|| anyhow::anyhow!("webauthn: origin has no host"))?;

    let webauthn = WebauthnBuilder::new(rp_id, &origin)
        .map_err(|e| anyhow::anyhow!("webauthn builder: {e}"))?
        .rp_name("TaskApp")
        .build()
        .map_err(|e| anyhow::anyhow!("webauthn build: {e}"))?;

    Ok(Arc::new(webauthn))
}

/// RP ID は設定値、無ければ origin のホスト名。
/// docker-compose の `${WEBAUTHN_RP_ID:-}` は未設定を空文字で渡すので、空は未設定として扱う
fn resolve_rp_id<'a>(configured: Option<&'a str>, origin: &'a Url) -> Option<&'a str> {
    configured
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .or_else(|| origin.host_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_rp_id_falls_back_to_origin_host() {
        let origin = Url::parse("https://task.example.com").unwrap();
        assert_eq!(resolve_rp_id(None, &origin), Some("task.example.com"));
        assert_eq!(resolve_rp_id(Some(""), &origin), Some("task.example.com"));
        assert_eq!(resolve_rp_id(Some("  "), &origin), Some("task.example.com"));
        assert_eq!(resolve_rp_id(Some("example.com"), &origin), Some("example.com"));
    }
}

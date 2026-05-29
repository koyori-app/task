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
        parsed.host_str()
            .ok_or_else(|| anyhow::anyhow!("webauthn: origin has no host"))?,
        parsed.port()
            .map(|p| format!(":{p}"))
            .unwrap_or_default()
    );
    Url::parse(&origin).map_err(|e| anyhow::anyhow!("webauthn origin: {e}"))
}

pub fn build_webauthn(settings: &Settings) -> Result<Arc<Webauthn>, anyhow::Error> {
    let origin = webauthn_origin(settings)?;
    let rp_id = settings
        .webauthn_rp_id
        .as_deref()
        .or_else(|| origin.host_str())
        .ok_or_else(|| anyhow::anyhow!("webauthn: origin has no host"))?;

    let webauthn = WebauthnBuilder::new(rp_id, &origin)
        .map_err(|e| anyhow::anyhow!("webauthn builder: {e}"))?
        .rp_name("TaskApp")
        .build()
        .map_err(|e| anyhow::anyhow!("webauthn build: {e}"))?;

    Ok(Arc::new(webauthn))
}

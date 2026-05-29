use std::sync::Arc;

use url::Url;
use webauthn_rs::prelude::*;

use crate::settings::Settings;

pub fn build_webauthn(settings: &Settings) -> Result<Arc<Webauthn>, anyhow::Error> {
    let origin = Url::parse(settings.email_verification_app_url.trim())?;
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

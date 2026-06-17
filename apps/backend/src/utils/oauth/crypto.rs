//! OAuth トークンの AES-256-GCM 暗号化。

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn encrypt_token(key: &[u8; 32], plaintext: &str) -> Result<String, anyhow::Error> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("aes key init: {e}"))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("aes encrypt: {e}"))?;

    let mut out = nonce_bytes.to_vec();
    out.extend(ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(out))
}

pub fn decrypt_token(key: &[u8; 32], encoded: &str) -> Result<String, anyhow::Error> {
    let data = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| anyhow::anyhow!("base64 decode: {e}"))?;

    if data.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("aes key init: {e}"))?;
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("aes decrypt: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("utf8: {e}"))
}

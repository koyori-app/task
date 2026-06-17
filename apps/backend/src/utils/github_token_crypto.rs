//! Installation Access Token の AES-256-GCM 暗号化。

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, anyhow};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::Rng;

const NONCE_LEN: usize = 12;

pub fn encrypt_token(key_material: &str, plaintext: &str) -> Result<String, anyhow::Error> {
    let key = derive_key(key_material)?;
    let cipher = Aes256Gcm::new(&key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("encrypt token: {e}"))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(out))
}

pub fn decrypt_token(key_material: &str, encoded: &str) -> Result<String, anyhow::Error> {
    let key = derive_key(key_material)?;
    let cipher = Aes256Gcm::new(&key.into());
    let bytes = STANDARD.decode(encoded).context("decode encrypted token")?;
    if bytes.len() <= NONCE_LEN {
        return Err(anyhow!("encrypted token too short"));
    }
    let (nonce_bytes, ciphertext) = bytes.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow!("decrypt token: {e}"))?;
    String::from_utf8(plaintext).context("token utf8")
}

fn derive_key(key_material: &str) -> Result<[u8; 32], anyhow::Error> {
    let bytes = key_material.as_bytes();
    if bytes.len() < 32 {
        return Err(anyhow!(
            "github_token_encryption_key must be at least 32 bytes"
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[..32]);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let key = "a".repeat(32);
        let enc = encrypt_token(&key, "ghs_test_token").unwrap();
        let dec = decrypt_token(&key, &enc).unwrap();
        assert_eq!(dec, "ghs_test_token");
    }
}

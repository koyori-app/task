//! Installation Access Token の AES-256-GCM 暗号化。
//!
//! 鍵導出は HKDF-SHA256（v2）。既存データとの互換のため、旧方式（先頭32バイト
//! 切り取りのみ、v1）で暗号化されたトークンも復号できる。新規暗号化は常に v2。
//!
//! フォーマット:
//! - v2: `"v2:" + base64(nonce(12B) + ciphertext)`
//! - v1（レガシー）: `base64(nonce(12B) + ciphertext)`（プレフィックスなし）

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, anyhow};
use base64::{Engine, engine::general_purpose::STANDARD};
use hkdf::Hkdf;
use rand::Rng;
use sha2::Sha256;

const NONCE_LEN: usize = 12;
const V2_PREFIX: &str = "v2:";
const HKDF_INFO: &[u8] = b"koyori-task/github-installation-token/v2";

pub fn encrypt_token(key_material: &str, plaintext: &str) -> Result<String, anyhow::Error> {
    let key = derive_key_v2(key_material)?;
    let encoded = encrypt_with_key(&key, plaintext)?;
    Ok(format!("{V2_PREFIX}{encoded}"))
}

pub fn decrypt_token(key_material: &str, encoded: &str) -> Result<String, anyhow::Error> {
    if let Some(body) = encoded.strip_prefix(V2_PREFIX) {
        let key = derive_key_v2(key_material)?;
        decrypt_with_key(&key, body)
    } else {
        // v1（レガシー）: 先頭32バイト切り取りの鍵導出で暗号化されたトークン。
        let key = derive_key_v1(key_material)?;
        decrypt_with_key(&key, encoded)
    }
}

/// 復号結果と、v2 への再暗号化が必要かどうか。
pub struct DecryptedToken {
    pub plaintext: String,
    pub needs_reencrypt: bool,
}

/// v1（レガシー）データを検出できる形で復号する。
///
/// 呼び出し側（DB からトークンを読み出して利用するコード）は `needs_reencrypt`
/// が `true` の場合、返り値の `plaintext` を `encrypt_token` で再暗号化して
/// 保存し直すこと（lazy migration）。
pub fn decrypt_token_for_migration(
    key_material: &str,
    encoded: &str,
) -> Result<DecryptedToken, anyhow::Error> {
    let needs_reencrypt = !encoded.starts_with(V2_PREFIX);
    let plaintext = decrypt_token(key_material, encoded)?;
    Ok(DecryptedToken {
        plaintext,
        needs_reencrypt,
    })
}

fn encrypt_with_key(key: &[u8; 32], plaintext: &str) -> Result<String, anyhow::Error> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("encrypt token: {e}"))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(out))
}

fn decrypt_with_key(key: &[u8; 32], encoded: &str) -> Result<String, anyhow::Error> {
    let cipher = Aes256Gcm::new(key.into());
    let bytes = STANDARD.decode(encoded).context("decode encrypted token")?;
    if bytes.len() <= NONCE_LEN {
        return Err(anyhow!("encrypted token too short"));
    }
    let (nonce_bytes, ciphertext) = bytes.split_at(NONCE_LEN);
    let nonce = Nonce::try_from(nonce_bytes).map_err(|_| anyhow!("invalid nonce length"))?;
    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| anyhow!("decrypt token: {e}"))?;
    String::from_utf8(plaintext).context("token utf8")
}

/// v2: HKDF-SHA256 による鍵導出。
fn derive_key_v2(key_material: &str) -> Result<[u8; 32], anyhow::Error> {
    check_key_material_len(key_material)?;
    let hk = Hkdf::<Sha256>::new(None, key_material.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(HKDF_INFO, &mut key)
        .map_err(|e| anyhow!("hkdf expand: {e}"))?;
    Ok(key)
}

/// v1（レガシー）: 先頭32バイト切り取りのみの鍵導出。既存暗号化データの復号専用で、
/// 新規暗号化には使わない。
fn derive_key_v1(key_material: &str) -> Result<[u8; 32], anyhow::Error> {
    check_key_material_len(key_material)?;
    let bytes = key_material.as_bytes();
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[..32]);
    Ok(key)
}

fn check_key_material_len(key_material: &str) -> Result<(), anyhow::Error> {
    if key_material.len() < 32 {
        return Err(anyhow!(
            "github_token_encryption_key must be at least 32 bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt_v2() {
        let key = "a".repeat(32);
        let enc = encrypt_token(&key, "ghs_test_token").unwrap();
        assert!(enc.starts_with(V2_PREFIX), "new ciphertexts must be v2");
        let dec = decrypt_token(&key, &enc).unwrap();
        assert_eq!(dec, "ghs_test_token");
    }

    #[test]
    fn decrypts_legacy_v1_ciphertext() {
        let key = "a".repeat(32);
        // v1 は先頭32バイト切り取りの鍵導出 + プレフィックスなし。
        let legacy_key = derive_key_v1(&key).unwrap();
        let legacy_enc = encrypt_with_key(&legacy_key, "ghs_legacy_token").unwrap();
        assert!(!legacy_enc.starts_with(V2_PREFIX));

        let dec = decrypt_token(&key, &legacy_enc).unwrap();
        assert_eq!(dec, "ghs_legacy_token");
    }

    #[test]
    fn decrypt_token_for_migration_flags_legacy_data() {
        let key = "a".repeat(32);
        let legacy_key = derive_key_v1(&key).unwrap();
        let legacy_enc = encrypt_with_key(&legacy_key, "ghs_legacy_token").unwrap();

        let migrated = decrypt_token_for_migration(&key, &legacy_enc).unwrap();
        assert_eq!(migrated.plaintext, "ghs_legacy_token");
        assert!(migrated.needs_reencrypt);

        let fresh_enc = encrypt_token(&key, "ghs_fresh_token").unwrap();
        let not_migrated = decrypt_token_for_migration(&key, &fresh_enc).unwrap();
        assert_eq!(not_migrated.plaintext, "ghs_fresh_token");
        assert!(!not_migrated.needs_reencrypt);
    }

    #[test]
    fn derive_key_v2_is_not_a_naive_truncation() {
        let key = "a".repeat(32);
        let v1 = derive_key_v1(&key).unwrap();
        let v2 = derive_key_v2(&key).unwrap();
        assert_ne!(v1, v2, "HKDF-derived key must differ from the raw prefix");
    }

    #[test]
    fn derive_key_v2_rejects_short_key_material() {
        let key = "a".repeat(31);
        assert!(derive_key_v2(&key).is_err());
    }
}

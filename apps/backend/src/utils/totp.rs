use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::Luma;
use qrcode::QrCode;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use sea_orm::prelude::Uuid;
use totp_rs::{Algorithm, Secret, TOTP};

use crate::utils::auth::{create_personal_token_hash, AuthError};

const NONCE_LEN: usize = 12;
const RECOVERY_CODE_COUNT: usize = 10;
const RECOVERY_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub fn encrypt_totp_secret(plain_secret: &str, key: &str) -> Result<String, AuthError> {
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("aes key: {e}")))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plain_secret.as_bytes())
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("aes encrypt: {e}")))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(out))
}

pub fn decrypt_totp_secret(secret_enc: &str, key: &str) -> Result<String, AuthError> {
    let data = BASE64
        .decode(secret_enc)
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("base64 decode: {e}")))?;
    if data.len() <= NONCE_LEN {
        return Err(AuthError::Internal(anyhow::anyhow!("invalid secret_enc")));
    }
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("aes key: {e}")))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("aes decrypt: {e}")))?;
    String::from_utf8(plain)
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("utf8 secret: {e}")))
}

pub fn generate_totp_secret_base32() -> Result<String, AuthError> {
    Ok(Secret::generate_secret().to_encoded().to_string())
}

pub fn build_totp(secret_base32: &str, issuer: &str, account: &str) -> Result<TOTP, AuthError> {
    let secret = Secret::Encoded(secret_base32.to_string())
        .to_bytes()
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("totp secret bytes: {e}")))?;
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret,
        Some(issuer.to_string()),
        account.to_string(),
    )
    .map_err(|e| AuthError::Internal(anyhow::anyhow!("totp init: {e}")))
}

pub fn verify_totp_code(
    secret_base32: &str,
    issuer: &str,
    account: &str,
    code: &str,
) -> Result<bool, AuthError> {
    let totp = build_totp(secret_base32, issuer, account)?;
    Ok(totp.check_current(code).unwrap_or(false))
}

pub fn otpauth_uri(secret_base32: &str, issuer: &str, account: &str) -> Result<String, AuthError> {
    let totp = build_totp(secret_base32, issuer, account)?;
    Ok(totp.get_url())
}

pub fn qr_code_png_data_uri(otpauth_uri: &str) -> Result<String, AuthError> {
    let code = QrCode::new(otpauth_uri.as_bytes())
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("qr encode: {e}")))?;
    let image = code.render::<Luma<u8>>().build();
    let mut png_bytes = Vec::new();
    image::DynamicImage::ImageLuma8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("png write: {e}")))?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(png_bytes)))
}

fn random_recovery_segment() -> String {
    let mut buf = [0u8; 4];
    OsRng.fill_bytes(&mut buf);
    buf.iter()
        .map(|b| RECOVERY_ALPHABET[(*b as usize) % RECOVERY_ALPHABET.len()] as char)
        .collect()
}

pub fn generate_recovery_code_plain() -> String {
    format!(
        "{}-{}-{}",
        random_recovery_segment(),
        random_recovery_segment(),
        random_recovery_segment()
    )
}

pub fn generate_recovery_codes() -> Vec<String> {
    (0..RECOVERY_CODE_COUNT)
        .map(|_| generate_recovery_code_plain())
        .collect()
}

pub fn hash_recovery_code(code: &str, secret: &str) -> Result<String, AuthError> {
    create_personal_token_hash(code, secret)
}

pub fn normalize_recovery_code(input: &str) -> String {
    input.trim().to_uppercase()
}

const ATTEMPT_KEY_PREFIX: &str = "2fa_attempts:";
const MAX_ATTEMPTS: i64 = 5;
const LOCKOUT_SECS: u64 = 900;

pub async fn check_2fa_lockout(
    redis: &crate::utils::redis::RedisConnection,
    user_id: Uuid,
) -> Result<(), AuthError> {
    let key = format!("{ATTEMPT_KEY_PREFIX}{user_id}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis acquire: {e}")))?;
    let count: Option<i64> = redis::cmd("GET")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis get attempts: {e}")))?;
    if count.unwrap_or(0) >= MAX_ATTEMPTS {
        return Err(AuthError::TooManyRequests);
    }
    Ok(())
}

pub async fn record_2fa_failure(
    redis: &crate::utils::redis::RedisConnection,
    user_id: Uuid,
) -> Result<(), AuthError> {
    let key = format!("{ATTEMPT_KEY_PREFIX}{user_id}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis acquire: {e}")))?;
    let count: i64 = redis::cmd("INCR")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis incr: {e}")))?;
    if count == 1 {
        let _: () = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(LOCKOUT_SECS)
            .query_async(&mut conn)
            .await
            .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis expire: {e}")))?;
    }
    Ok(())
}

pub async fn clear_2fa_attempts(
    redis: &crate::utils::redis::RedisConnection,
    user_id: Uuid,
) -> Result<(), AuthError> {
    let key = format!("{ATTEMPT_KEY_PREFIX}{user_id}");
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis acquire: {e}")))?;
    let _: () = redis::cmd("DEL")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("redis del: {e}")))?;
    Ok(())
}

pub fn recovery_code_matches(stored_hash: &str, candidate_hash: &str) -> bool {
    use subtle::ConstantTimeEq;
    stored_hash
        .as_bytes()
        .ct_eq(candidate_hash.as_bytes())
        .into()
}

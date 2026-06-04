//! PKCE (RFC 7636) ヘルパ。

use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
}

/// 32 バイト乱数の code_verifier と S256 code_challenge を生成する。
pub fn generate_pkce_pair() -> PkcePair {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    let code_verifier = URL_SAFE_NO_PAD.encode(buf);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);
    PkcePair {
        code_verifier,
        code_challenge,
    }
}

/// OAuth state パラメータ（16 バイト乱数, base64url）。
pub fn generate_state() -> String {
    let mut buf = [0u8; 16];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

use argon2::{
    Argon2,
    password_hash::{
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
        rand_core::{OsRng, RngCore},
    },
};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use thiserror::Error;
use tracing::debug;
use utoipa::ToSchema;

/// API 共通のエラー応答ボディ。

#[derive(Serialize, ToSchema)]
pub struct ServerError {
    #[schema(example = "invalid-credentials")]
    pub message: String,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("forbidden")]
    Forbidden,
    #[error("email not verified")]
    EmailNotVerified,
    #[error("invalid verification token")]
    InvalidVerificationToken,
    #[error("no such user")]
    UserNotFound,
    #[error("email already verified")]
    EmailAlreadyVerified,
    #[error("duplicate email")]
    DuplicateEmail,
    #[error("too many requests")]
    TooManyRequests,
}

impl From<sea_orm::DbErr> for AuthError {
    fn from(err: sea_orm::DbErr) -> Self {
        AuthError::Internal(err.into())
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::Internal(e) => {
                debug!("auth error: {:#?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ServerError {
                        message: "internal-error".into(),
                    }),
                )
                    .into_response()
            }
            AuthError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(ServerError {
                    message: "unauthorized".into(),
                }),
            )
                .into_response(),
            AuthError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                Json(ServerError {
                    message: "invalid-credentials".into(),
                }),
            )
                .into_response(),
            AuthError::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "forbidden".into(),
                }),
            )
                .into_response(),
            AuthError::EmailNotVerified => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "email-not-verified".into(),
                }),
            )
                .into_response(),
            AuthError::InvalidVerificationToken => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "invalid-verification-token".into(),
                }),
            )
                .into_response(),
            AuthError::UserNotFound => (
                StatusCode::NOT_FOUND,
                Json(ServerError {
                    message: "not-found".into(),
                }),
            )
                .into_response(),
            AuthError::EmailAlreadyVerified => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "email-already-verified".into(),
                }),
            )
                .into_response(),
            AuthError::DuplicateEmail => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "email-already-exists".into(),
                }),
            )
                .into_response(),
            AuthError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ServerError {
                    message: "too-many-requests".into(),
                }),
            )
                .into_response(),
        }
    }
}

pub fn argon2_params() -> Result<Argon2<'static>, AuthError> {
    // Argon2idパラメータ
    let params = argon2::Params::new(
        131072, // memory cost
        3,      // time cost
        2,      // parallelism
        None,   // output length
    )
    .map_err(|e| AuthError::Internal(anyhow::anyhow!("argon2 params: {e}")))?;

    Ok(Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    ))
}

/// パスワードをハッシュ化する関数
///
/// Argon2idアルゴリズムを使用し、ランダムなソルトを生成してハッシュ化します。
///
/// # Arguments
/// * `password` - ハッシュ化するパスワードの文字列
///
/// # Errors
/// * `AuthError::Internal` - ハッシュ化プロセスでエラーが発生した場合に返されます。
///
/// # Returns
/// * `Ok(String)` - ハッシュ化されたパスワードを含む文字
pub fn create_password_hash(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = argon2_params()?;

    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("password hash: {e}")))?;

    Ok(hash.to_string())
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, AuthError> {
    let parsed_hash = PasswordHash::new(password_hash)
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("invalid password hash: {e}")))?;

    let argon2 = argon2_params()?;
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// メール認証用トークンを生成する。
pub fn generate_email_verification_token() -> String {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

// --- Personal token helpers ---
type HmacSha256 = Hmac<Sha256>;

/// 生成したトークン本体と、それをDBに保存するためのハッシュを返す。
/// トークン本体はランダムなバイト列をBase64URLでエンコードしたもの。
pub fn generate_personal_token() -> Result<(String, String), AuthError> {
    // 32バイトのランダム値
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    let token = URL_SAFE_NO_PAD.encode(&buf);

    let token_hash = create_personal_token_hash(&token)?;
    Ok((token, token_hash))
}

/// サーバー側で保持するトークンのハッシュを作る。
/// 簡易的には HMAC-SHA256(secret, token) を Base64URL でエンコードして保存する。
pub fn create_personal_token_hash(token: &str) -> Result<String, AuthError> {
    let secret = std::env::var("PERSONAL_TOKEN_SECRET")
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("token secret missing: {e}")))?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("hmac init: {e}")))?;
    mac.update(token.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(URL_SAFE_NO_PAD.encode(result.as_slice()))
}

/// 受信したトークンを、DB にある `stored_hash` と比較して検証する。
pub fn verify_personal_token(token: &str, stored_hash: &str) -> Result<bool, AuthError> {
    let computed = create_personal_token_hash(token)?;

    let computed_bytes = computed.as_bytes();
    let stored_bytes = stored_hash.as_bytes();

    // 長さが違う場合でも、すぐに false を返さずダミーの比較を行うか、
    // そもそもハッシュ関数の出力なので長さが同じはずであることを前提にする。
    if computed_bytes.len() != stored_bytes.len() {
        return Ok(false);
    }

    Ok(computed_bytes.ct_eq(stored_bytes).into())
}

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use tracing::debug;

#[derive(Serialize)]
pub struct ServerError {
    pub message: String,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    #[error("forbidden")]
    Forbidden,
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
            AuthError::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "forbidden".into(),
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

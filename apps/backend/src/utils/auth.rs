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
use sha2::Sha256;
use thiserror::Error;
use tracing::debug;

use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::entities::personal_tokens::{self, Entity as PersonalTokenEntity};
use crate::error::{ServerError, internal_server_error};
use sea_orm::DatabaseConnection;

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
    #[error("account suspended")]
    Suspended,
    /// 認証メールジョブのキュー投入に失敗した（未認証ユーザーは残し再送 API で回復する）。
    #[error("verification email enqueue failed")]
    VerificationEmailEnqueueFailed(#[source] anyhow::Error),
    #[error("invalid 2fa code")]
    InvalidTwoFactorCode,
    #[error("2fa already enabled")]
    TwoFactorAlreadyEnabled,
    #[error("2fa not enabled")]
    TwoFactorNotEnabled,
    #[error("password reset email enqueue failed")]
    PasswordResetEmailEnqueueFailed(#[source] anyhow::Error),
    #[error("invalid password reset token")]
    InvalidPasswordResetToken,
    #[error("password reset token not found")]
    PasswordResetTokenNotFound,
    #[error("invalid current password")]
    InvalidCurrentPassword,
    #[error("password not set")]
    PasswordNotSet,
    #[error("invalid new password")]
    InvalidNewPassword,
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
                internal_server_error().into_response()
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
            AuthError::Suspended => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "account-suspended".into(),
                }),
            )
                .into_response(),
            AuthError::VerificationEmailEnqueueFailed(e) => {
                debug!("verification email enqueue failed: {:#?}", e);
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ServerError {
                        message: "verification-email-enqueue-failed".into(),
                    }),
                )
                    .into_response()
            }
            AuthError::InvalidTwoFactorCode => (
                StatusCode::UNAUTHORIZED,
                Json(ServerError {
                    message: "invalid-2fa-code".into(),
                }),
            )
                .into_response(),
            AuthError::TwoFactorAlreadyEnabled => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "2fa-already-enabled".into(),
                }),
            )
                .into_response(),
            AuthError::TwoFactorNotEnabled => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "2fa-not-enabled".into(),
                }),
            )
                .into_response(),
            AuthError::PasswordResetEmailEnqueueFailed(e) => {
                debug!("password reset email enqueue failed: {:#?}", e);
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ServerError {
                        message: "password-reset-email-enqueue-failed".into(),
                    }),
                )
                    .into_response()
            }
            AuthError::InvalidPasswordResetToken => (
                StatusCode::BAD_REQUEST,
                Json(ServerError { message: "invalid-password-reset-token".into() }),
            )
                .into_response(),
            AuthError::PasswordResetTokenNotFound => (
                StatusCode::NOT_FOUND,
                Json(ServerError { message: "password-reset-token-not-found".into() }),
            )
                .into_response(),
            AuthError::InvalidCurrentPassword => (
                StatusCode::BAD_REQUEST,
                Json(ServerError { message: "invalid-current-password".into() }),
            )
                .into_response(),
            AuthError::PasswordNotSet => (
                StatusCode::BAD_REQUEST,
                Json(ServerError { message: "password-not-set".into() }),
            )
                .into_response(),
            AuthError::InvalidNewPassword => (
                StatusCode::BAD_REQUEST,
                Json(ServerError { message: "invalid-new-password".into() }),
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

/// 存在しないユーザー向けのダミーハッシュ。ログイン時に常に Argon2 検証を走らせ、
/// メールアドレスの有無による応答時間差（タイミング攻撃）を抑える。
pub const DUMMY_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=131072,t=3,p=2$0UUArODQDWduujvFlpWtKg$GDp6SlCwV4PIue/EfTr+nJVjlFnycyxtCfnJMnjlIjU";

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

/// `pat_<base64url>` 形式のトークンと、DBに保存するHMACハッシュを返す。
pub fn generate_personal_token(secret: &str) -> Result<(String, String), AuthError> {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    let token = format!("pat_{}", URL_SAFE_NO_PAD.encode(&buf));
    let token_hash = create_personal_token_hash(&token, secret)?;
    Ok((token, token_hash))
}

/// サーバー側で保持するトークンのハッシュを作る。
/// HMAC-SHA256(secret, token) を Base64URL でエンコードして返す。
pub fn create_personal_token_hash(token: &str, secret: &str) -> Result<String, AuthError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("hmac init: {e}")))?;
    mac.update(token.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(URL_SAFE_NO_PAD.encode(result.as_slice()))
}

/// DB から取得した PAT レコード（認証成功時）。
pub type PersonalTokenRecord = personal_tokens::Model;

/// Bearer トークンを検証し、有効な PAT レコードを返す。
pub async fn authenticate_personal_token(
    db: &DatabaseConnection,
    secret: &str,
    token_plaintext: &str,
) -> Result<PersonalTokenRecord, AuthError> {
    let token_hash = create_personal_token_hash(token_plaintext, secret)?;

    let token = PersonalTokenEntity::find()
        .filter(personal_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or(AuthError::Unauthorized)?;

    if token.revoked {
        return Err(AuthError::Unauthorized);
    }

    if let Some(expires) = &token.expires_at {
        if expires < &Utc::now().fixed_offset() {
            return Err(AuthError::Unauthorized);
        }
    }

    Ok(token)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_password_hash_is_valid_argon2() {
        assert!(PasswordHash::new(DUMMY_PASSWORD_HASH).is_ok());
        assert!(!verify_password("wrong-password", DUMMY_PASSWORD_HASH).unwrap());
    }
}

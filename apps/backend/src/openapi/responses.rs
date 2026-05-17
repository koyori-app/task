//! OpenAPI ドキュメント専用のレスポンス型（実行時には `AuthError` を使用）。

#![allow(dead_code)]

use utoipa::IntoResponses;

use crate::utils::auth::ServerError;

/// セッション認証必須 API の共通エラー（401 / 403 / 500）
#[derive(IntoResponses)]
pub enum SessionAuthErrors {
    #[response(status = 401, description = "Unauthorized")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 403, description = "Forbidden")]
    Forbidden(#[to_schema] ServerError),
    #[response(status = 500, description = "Internal server error")]
    Internal(#[to_schema] ServerError),
}

/// ログイン等、認証前 API のエラー（403 / 500）
#[derive(IntoResponses)]
pub enum CredentialErrors {
    #[response(status = 403, description = "Forbidden")]
    Forbidden(#[to_schema] ServerError),
    #[response(status = 500, description = "Internal server error")]
    Internal(#[to_schema] ServerError),
}

/// 認証必須だが 403 を返さない API のエラー（401 / 500）
#[derive(IntoResponses)]
pub enum UnauthorizedErrors {
    #[response(status = 401, description = "Unauthorized")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 500, description = "Internal server error")]
    Internal(#[to_schema] ServerError),
}

/// 内部エラーのみ（500）
#[derive(IntoResponses)]
#[response(status = 500, description = "Internal server error")]
pub struct InternalOnlyError(#[to_schema] ServerError);

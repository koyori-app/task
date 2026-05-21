//! OpenAPI 用の共通レスポンス型（ランタイムの `IntoResponse` とは別定義）。

#![allow(dead_code)]

use utoipa::IntoResponses;

use crate::error::ServerError;

#[derive(IntoResponses)]
pub enum SessionAuthErrors {
    #[response(status = 401, description = "ログインまたはセッションが必要です")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 403, description = "この操作は許可されていません")]
    Forbidden(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum CredentialErrors {
    #[response(
        status = 401,
        description = "メールアドレスまたはパスワードが正しくありません"
    )]
    InvalidCredentials(#[to_schema] ServerError),
    #[response(
        status = 403,
        description = "メールアドレスの確認が済んでいないためログインできません"
    )]
    EmailNotVerified(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum RegisterErrors {
    #[response(
        status = 409,
        description = "このメールアドレスはすでに登録されています"
    )]
    Conflict(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum VerifyEmailErrors {
    #[response(
        status = 400,
        description = "認証用リンクが無効か、または有効期限切れです"
    )]
    InvalidToken(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum UnauthorizedErrors {
    #[response(status = 401, description = "ログインまたはセッションが必要です")]
    Unauthorized(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
#[response(
    status = 500,
    description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
)]
pub struct InternalOnlyError(#[to_schema] ServerError);

#[derive(IntoResponses)]
pub enum ResendVerificationErrors {
    #[response(
        status = 404,
        description = "入力されたメールアドレスのアカウントが見つかりませんでした"
    )]
    NotFound(#[to_schema] ServerError),
    #[response(
        status = 409,
        description = "このアカウントではメール認証はもう完了しています"
    )]
    Conflict(#[to_schema] ServerError),
    #[response(status = 429, description = "しばらくしてから再度お試しください")]
    TooManyRequests(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

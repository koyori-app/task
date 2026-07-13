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
    // #26: メールアドレス列挙対策のため、既存メールアドレスでの登録も 409 ではなく
    // 未使用時と同一の 201 を返す（Conflict は意図的に定義しない）。
    #[response(
        status = 429,
        description = "同一メールアドレスへの登録リクエストが連続しています。時間をおいて再度お試しください"
    )]
    TooManyRequests(#[to_schema] ServerError),
    #[response(
        status = 503,
        description = "確認/通知メールの送信準備に失敗しました。時間をおいて再度お試しください"
    )]
    VerificationEmailUnavailable(#[to_schema] ServerError),
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

/// テナント作成専用のエラー。`display_id` の重複で 409 を返し得るため
/// `CrudErrors` から 404 を除き 409 を足した形状。
#[derive(IntoResponses)]
pub enum TenantCreateErrors {
    #[response(status = 401, description = "ログインまたはセッションが必要です")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 403, description = "この操作は許可されていません")]
    Forbidden(#[to_schema] ServerError),
    #[response(status = 409, description = "指定した表示IDはすでに使用されています")]
    Conflict(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum CrudErrors {
    #[response(status = 401, description = "ログインまたはセッションが必要です")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 403, description = "この操作は許可されていません")]
    Forbidden(#[to_schema] ServerError),
    #[response(status = 404, description = "リソースが見つかりません")]
    NotFound(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum DriveFolderErrors {
    #[response(status = 401, description = "ログインまたはセッションが必要です")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 403, description = "この操作は許可されていません")]
    Forbidden(#[to_schema] ServerError),
    #[response(status = 404, description = "リソースが見つかりません")]
    NotFound(#[to_schema] ServerError),
    #[response(
        status = 409,
        description = "フォルダ内にファイルまたはサブフォルダが存在します"
    )]
    Conflict(#[to_schema] ServerError),
    #[response(
        status = 422,
        description = "リクエストは受理できません（例: editor 権限は Phase 2 以降）"
    )]
    UnprocessableEntity(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum PublicShareErrors {
    #[response(status = 404, description = "共有リンクが見つかりません")]
    NotFound(#[to_schema] ServerError),
    #[response(status = 410, description = "共有リンクの有効期限が切れています")]
    Gone(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum OAuthErrors {
    #[response(status = 302, description = "リダイレクト")]
    Redirect,
    #[response(status = 400, description = "リクエストが不正です")]
    BadRequest(#[to_schema] ServerError),
    #[response(status = 401, description = "ログインまたはセッションが必要です")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 403, description = "この操作は許可されていません")]
    Forbidden(#[to_schema] ServerError),
    #[response(status = 404, description = "リソースが見つかりません")]
    NotFound(#[to_schema] ServerError),
    #[response(status = 409, description = "競合（メール重複・連携済み等）")]
    Conflict(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

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
        status = 503,
        description = "認証メールの送信準備に失敗しました。しばらくしてから再送をお試しください"
    )]
    VerificationEmailUnavailable(#[to_schema] ServerError),
    #[response(
        status = 500,
        description = "サーバー側で問題が発生しました。時間をおいて再度お試しください"
    )]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum PasswordResetRequestErrors {
    #[response(status = 400, description = "リクエストが不正です")]
    BadRequest(#[to_schema] ServerError),
    #[response(status = 429, description = "しばらくしてから再度お試しください")]
    TooManyRequests(#[to_schema] ServerError),
    #[response(status = 500, description = "サーバー側で問題が発生しました")]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum PasswordResetVerifyErrors {
    #[response(status = 404, description = "トークンが無効または期限切れです")]
    NotFound(#[to_schema] ServerError),
    #[response(status = 500, description = "サーバー側で問題が発生しました")]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum PasswordResetCompleteErrors {
    #[response(status = 400, description = "リクエストが不正です")]
    BadRequest(#[to_schema] ServerError),
    #[response(status = 500, description = "サーバー側で問題が発生しました")]
    Internal(#[to_schema] ServerError),
}

#[derive(IntoResponses)]
pub enum PasswordChangeErrors {
    #[response(status = 401, description = "ログインが必要です")]
    Unauthorized(#[to_schema] ServerError),
    #[response(status = 400, description = "リクエストが不正です")]
    BadRequest(#[to_schema] ServerError),
    #[response(status = 500, description = "サーバー側で問題が発生しました")]
    Internal(#[to_schema] ServerError),
}

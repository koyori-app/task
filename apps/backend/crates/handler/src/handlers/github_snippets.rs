//! 連携リポジトリのコードの抜粋（`GET /v1/github/snippet`）。

use axum::{
    Json,
    extract::{Query, State},
    http::header,
    response::IntoResponse,
};

use crate::AppState;
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use payload::github::{GithubSnippetQuery, GithubSnippetResponse};
use service::github::snippets::{SnippetOutcome, find_integrations, parse_request, snippet_for};

/// 本文に貼った GitHub のパーマリンクが指す行を、連携リポジトリから返す。
// 見せる相手は「そのリポジトリを連携しているプロジェクトのどれかに入れる利用者」。
// 画面の描画箇所（タスク説明・コメント・ドキュメント・レビュー指摘）はプロジェクトの UUID を
// 知らないので、利用者を基準に連携行を探し、候補ごとに `ensure_tenant_access` を通す。
// 未連携と権限なしは区別せず 404 にする（連携の有無を外へ漏らさない）。
// PAT は通さない。テナントとプロジェクトに束縛されたトークンに、束縛の外かもしれない
// プロジェクトの連携を横断で引かせないため。
// `Cache-Control` を `private` にするのは認証付きの応答を共有キャッシュに置かせないためで、
// 5 分に留めるのは、権限を失った後にブラウザーに残る時間を短くするため。
#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/snippet",
    tag = "GitHub",
    summary = "連携リポジトリのコードの抜粋",
    params(GithubSnippetQuery),
    responses(
        (status = 200, description = "抜粋の行", body = GithubSnippetResponse),
        (status = 400, description = "クエリの形式が不正", body = ServerError),
        (status = 422, description = "行として表示できないファイル（バイナリ・大きすぎる）", body = ServerError),
        (status = 429, description = "取得回数の上限を超えた", body = ServerError),
        CrudErrors
    )
)]
pub async fn get_github_snippet(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<GithubSnippetQuery>,
) -> Result<impl IntoResponse, AppError> {
    let github = state.settings.require_github_app()?;
    auth.require_session_or_device_token()?;
    let request = parse_request(&query).ok_or(AppError::BadRequest)?;

    let mut allowed = None;
    for (integration, tenant_id) in
        find_integrations(&state.db, &request.owner, &request.repo).await?
    {
        match auth
            .ensure_tenant_access(&state, tenant_id, Some(integration.project_id))
            .await
        {
            Ok(()) => {
                allowed = Some(integration);
                break;
            }
            Err(AppError::Forbidden | AppError::ForbiddenDetail(_) | AppError::NotFound) => {}
            Err(e) => return Err(e),
        }
    }
    let integration = allowed.ok_or(AppError::NotFound)?;

    let outcome = snippet_for(
        &state.redis_client,
        &state.http_client,
        github,
        auth.user_id,
        &integration,
        &request,
    )
    .await
    .map_err(AppError::Internal)?;
    match outcome {
        SnippetOutcome::Found(snippet) => Ok((
            [(header::CACHE_CONTROL, "private, max-age=300")],
            Json(snippet),
        )),
        SnippetOutcome::NotFound => Err(AppError::NotFound),
        SnippetOutcome::NotDisplayable => Err(AppError::UnprocessableEntity),
        SnippetOutcome::RateLimited => Err(AppError::TooManyRequests),
    }
}

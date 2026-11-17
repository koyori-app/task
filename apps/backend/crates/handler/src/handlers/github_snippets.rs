//! 連携リポジトリのコードの抜粋（`GET /v1/github/snippet`）。

use axum::{
    Json,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
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
// 応答は `Cache-Control: no-store` にする。見せてよいかは利用者で変わるのに URL は同じなので、
// ブラウザーの HTTP キャッシュに残すと、同じブラウザーで次にログインした別の利用者がこの認可を
// 通らずに前の利用者の抜粋を受け取る（`private` は共有キャッシュを止めるだけで、利用者は分けない）。
// 取り直しの費用は、認可の後ろにある Redis のキャッシュが受ける。
// GitHub 側の失敗（レート制限・障害・タイムアウト）は 500 ではなく 503 + `Retry-After` で返す。
// こちらの不具合ではないので、エラー監視を内部エラーで埋めない。
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
        (status = 503, description = "GitHub から取得できない（レート制限・障害）。`Retry-After` の秒数の間は取りに行かない", body = ServerError),
        CrudErrors
    )
)]
pub async fn get_github_snippet(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<GithubSnippetQuery>,
) -> Response {
    let mut response = match snippet_response(&state, &auth, &query).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    };
    // 拒否（404 など）も含めて残させない。権限が付いた後に古い拒否が返るのも、その逆も防ぐ
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

async fn snippet_response(
    state: &AppState,
    auth: &AuthUser,
    query: &GithubSnippetQuery,
) -> Result<Response, AppError> {
    let github = state.settings.require_github_app()?;
    auth.require_session_or_device_token()?;
    let request = parse_request(query).ok_or(AppError::BadRequest)?;

    let mut allowed = None;
    for (integration, tenant_id) in
        find_integrations(&state.db, auth.user_id, &request.owner, &request.repo).await?
    {
        match auth
            .ensure_tenant_access(state, tenant_id, Some(integration.project_id))
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
        SnippetOutcome::Found(snippet) => Ok(Json(snippet).into_response()),
        SnippetOutcome::Unavailable { retry_after_secs } => Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::RETRY_AFTER, retry_after_secs.to_string())],
            Json(ServerError {
                message: "github-unavailable".into(),
            }),
        )
            .into_response()),
        SnippetOutcome::NotFound => Err(AppError::NotFound),
        SnippetOutcome::NotDisplayable => Err(AppError::UnprocessableEntity),
        SnippetOutcome::RateLimited => Err(AppError::TooManyRequests),
    }
}

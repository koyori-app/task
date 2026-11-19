//! MCP の OAuth の同意画面と、連携中のアプリの管理（セッション専用）。
//! 仕様は docs/dev/features/mcp.md の「OAuth での接続」。
//!
//! 認可の要求の検査に落ちたときは、クライアントへリダイレクトしない。画面にエラーを出して止まる
//! （リダイレクトするのは、検査を通った要求を利用者が承認・拒否したときだけ）。

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use chrono::Utc;
use reqwest::Url;
use sea_orm::prelude::Uuid;

use crate::AppState;
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::handlers::personal_tokens::ensure_can_bind_tenants;
use crate::openapi::{CrudErrors, SessionAuthErrors};
use common::validation::PKCE_CHALLENGE_REGEX;
use entity::scopes::Scope;
use payload::oauth_grants::*;
use service::oauth_server::client::{ClientError, ClientMetadata, resolve_client};
use service::oauth_server::grant::{self, PendingAuthorization};
use service::oauth_server::{
    issuer, parse_scope_param, redirect_uri_is_loopback, redirect_uri_matches, resource_matches,
};

fn bad_request(message: &'static str) -> AppError {
    AppError::BadRequestDetail(message.into())
}

fn client_error(error: ClientError) -> AppError {
    match error {
        ClientError::NotAllowed => bad_request("oauth-client-not-allowed"),
        ClientError::InvalidClientId | ClientError::InvalidDocument => {
            bad_request("oauth-invalid-client")
        }
        ClientError::Unavailable => bad_request("oauth-client-metadata-unavailable"),
        ClientError::Internal(error) => AppError::Internal(error),
    }
}

/// 認可の要求を確かめ、クライアントのメタデータと、渡せる scope を返す。
/// preview・承認・拒否が同じ検査を通る。
async fn validate_request(
    state: &AppState,
    user_id: Uuid,
    params: &AuthorizationParams,
) -> Result<(ClientMetadata, Vec<Scope>), AppError> {
    let client = resolve_client(
        &state.redis_client,
        &state.settings,
        user_id,
        &params.client_id,
    )
    .await
    .map_err(client_error)?;
    if !redirect_uri_matches(&client.redirect_uris, &params.redirect_uri) {
        return Err(bad_request("oauth-invalid-redirect-uri"));
    }
    if params.response_type != "code" {
        return Err(bad_request("oauth-unsupported-response-type"));
    }
    if params.code_challenge_method != "S256"
        || !PKCE_CHALLENGE_REGEX.is_match(&params.code_challenge)
    {
        return Err(bad_request("oauth-invalid-code-challenge"));
    }
    // `resource` の無い要求は通す（古い改訂のクライアント）。発行するトークンは常に MCP の入口宛て
    if let Some(resource) = params.resource.as_deref()
        && !resource_matches(&state.settings, resource)
    {
        return Err(bad_request("oauth-invalid-target"));
    }
    let scopes = parse_scope_param(params.scope.as_deref());
    if scopes.is_empty() {
        return Err(bad_request("oauth-invalid-scope"));
    }
    Ok((client, scopes))
}

/// 検査を通った `redirect_uri` に、応答の項目を query で足す。`iss` は常に付ける（RFC 9207）。
fn redirect_with(
    state: &AppState,
    params: &AuthorizationParams,
    result: (&str, &str),
) -> Result<String, AppError> {
    let mut url =
        Url::parse(&params.redirect_uri).map_err(|_| bad_request("oauth-invalid-redirect-uri"))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair(result.0, result.1);
        if let Some(state_param) = params.state.as_deref() {
            query.append_pair("state", state_param);
        }
        query.append_pair("iss", &issuer(&state.settings));
    }
    Ok(url.into())
}

/// `host` か、ポートが明示されていれば `host:port`。
fn host_with_explicit_port(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

fn preview_of(
    params: &AuthorizationParams,
    client: ClientMetadata,
    scopes: Vec<Scope>,
) -> Result<AuthorizationPreviewResponse, AppError> {
    let client_host = Url::parse(&client.client_id)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .ok_or_else(|| bad_request("oauth-invalid-client"))?;
    let redirect =
        Url::parse(&params.redirect_uri).map_err(|_| bad_request("oauth-invalid-redirect-uri"))?;
    Ok(AuthorizationPreviewResponse {
        client_id: client.client_id,
        client_name: client.client_name,
        client_host,
        redirect_host: host_with_explicit_port(&redirect),
        redirect_is_loopback: redirect_uri_is_loopback(&params.redirect_uri),
        scopes,
    })
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/preview",
    tag = "OAuth Grants",
    summary = "認可の要求を確かめ、同意画面に出す中身を返す",
    description = "セッション専用。要求が受けられないときは 400 で、`message` に理由（`oauth-client-not-allowed`・`oauth-invalid-client`・`oauth-client-metadata-unavailable`・`oauth-invalid-redirect-uri`・`oauth-unsupported-response-type`・`oauth-invalid-code-challenge`・`oauth-invalid-target`・`oauth-invalid-scope`）を返す。",
    request_body = AuthorizationParams,
    responses(
        (status = 200, description = "同意画面に出す中身", body = AuthorizationPreviewResponse),
        (status = 400, description = "認可の要求を受けられません", body = ServerError),
        SessionAuthErrors,
    )
)]
pub async fn preview_authorization(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(params)): Valid<Json<AuthorizationParams>>,
) -> Result<Json<AuthorizationPreviewResponse>, AppError> {
    auth.require_session()?;
    let (client, scopes) = validate_request(&state, auth.user_id, &params).await?;
    Ok(Json(preview_of(&params, client, scopes)?))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/authorize",
    tag = "OAuth Grants",
    summary = "認可の要求を承認し、認可コード付きの遷移先を返す",
    description = "セッション専用。選んだすべてのテナントで、所属と 2FA を確かめる（PAT の発行と同じ規則）。1 つでも通らなければ全体を拒否し、コードは発行しない。",
    request_body = ApproveAuthorizationRequest,
    responses(
        (status = 200, description = "クライアントのコールバック（`code`・`state`・`iss` 付き）", body = AuthorizationRedirectResponse),
        (status = 400, description = "認可の要求を受けられません", body = ServerError),
        CrudErrors,
    )
)]
pub async fn approve_authorization(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(payload)): Valid<Json<ApproveAuthorizationRequest>>,
) -> Result<Json<AuthorizationRedirectResponse>, AppError> {
    auth.require_session()?;
    let (client, requested) = validate_request(&state, auth.user_id, &payload.params).await?;
    // 要求された scope の部分集合だけを渡す。重複はまとめ、順は要求の側に合わせる
    if payload
        .scopes
        .iter()
        .any(|scope| !requested.contains(scope))
    {
        return Err(AppError::BadRequest);
    }
    let scopes: Vec<Scope> = requested
        .into_iter()
        .filter(|scope| payload.scopes.contains(scope))
        .collect();
    let tenant_ids = ensure_can_bind_tenants(&state, auth.user_id, &payload.tenant_ids).await?;

    let code = grant::issue_code(
        &state.redis_client,
        &PendingAuthorization {
            grant_id: Uuid::new_v4(),
            user_id: auth.user_id,
            client_id: client.client_id,
            client_name: client.client_name,
            redirect_uri: payload.params.redirect_uri.clone(),
            code_challenge: payload.params.code_challenge.clone(),
            scopes,
            tenant_ids,
            allow_refresh: client.allow_refresh,
            issued_at_ms: Utc::now().timestamp_millis(),
        },
    )
    .await?;
    Ok(Json(AuthorizationRedirectResponse {
        redirect_to: redirect_with(&state, &payload.params, ("code", &code))?,
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/deny",
    tag = "OAuth Grants",
    summary = "認可の要求を拒否し、エラー付きの遷移先を返す",
    description = "セッション専用。検査を通った要求にだけ遷移先を返す（確かめていない `redirect_uri` へ画面を飛ばさない）。",
    request_body = AuthorizationParams,
    responses(
        (status = 200, description = "クライアントのコールバック（`error=access_denied`・`state`・`iss` 付き）", body = AuthorizationRedirectResponse),
        (status = 400, description = "認可の要求を受けられません", body = ServerError),
        SessionAuthErrors,
    )
)]
pub async fn deny_authorization(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(params)): Valid<Json<AuthorizationParams>>,
) -> Result<Json<AuthorizationRedirectResponse>, AppError> {
    auth.require_session()?;
    validate_request(&state, auth.user_id, &params).await?;
    Ok(Json(AuthorizationRedirectResponse {
        redirect_to: redirect_with(&state, &params, ("error", "access_denied"))?,
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "OAuth Grants",
    summary = "連携中のアプリ（自分の OAuth の許可）の一覧",
    description = "セッション専用。取り消し済み・期限切れ・テナントの束縛が尽きたもの・サーバーの許可リストに無いクライアントのものは出ない。",
    responses(
        (status = 200, description = "使える許可の一覧", body = Vec<OAuthGrantResponse>),
        SessionAuthErrors,
    )
)]
pub async fn list_oauth_grants(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<OAuthGrantResponse>>, AppError> {
    auth.require_session()?;
    let grants = grant::list_grants(&state.db, &state.settings, auth.user_id).await?;
    let resp = grants
        .into_iter()
        .filter_map(|(grant, tenant_ids)| {
            Some(OAuthGrantResponse {
                id: grant.id,
                client_id: grant.oauth_client_id?,
                client_name: grant.name,
                tenant_ids,
                scopes: grant.scopes,
                expires_at: grant.expires_at?.with_timezone(&Utc),
                last_used_at: grant.last_used_at.map(|value| value.with_timezone(&Utc)),
            })
        })
        .collect();
    Ok(Json(resp))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "OAuth Grants",
    summary = "連携中のアプリの接続を解除",
    description = "セッション専用。許可を取り消し、そのアクセストークンとリフレッシュトークンを使えなくする。取り消し済みでも 204。",
    params(("id" = Uuid, Path, description = "許可の識別子")),
    responses(
        (status = 204, description = "解除しました"),
        CrudErrors,
    )
)]
pub async fn revoke_oauth_grant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    auth.require_session()?;
    let grant = grant::find_owned_grant(&state.db, auth.user_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    grant::revoke_grant(&state.db, grant.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_host_shows_only_an_explicit_port() {
        let host = |raw: &str| host_with_explicit_port(&Url::parse(raw).unwrap());
        assert_eq!(host("https://claude.ai/api/mcp/auth_callback"), "claude.ai");
        assert_eq!(host("https://claude.ai:443/cb"), "claude.ai");
        assert_eq!(host("http://127.0.0.1:53124/callback"), "127.0.0.1:53124");
        assert_eq!(host("http://localhost/callback"), "localhost");
        assert_eq!(host("http://[::1]:53124/callback"), "[::1]:53124");
    }
}

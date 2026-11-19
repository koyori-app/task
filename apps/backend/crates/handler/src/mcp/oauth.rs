//! MCP の OAuth の公開のエンドポイント（メタデータ 2 つ、トークン、失効）。
//!
//! どれも Cookie を見ない。資格はコードかトークンそのもので、クライアントは公開クライアント
//! （認証方式 `none`）だけを受ける。`openapi.json` には載せない（`/v1` の外）。
//! 仕様は docs/dev/features/mcp.md の「OAuth での接続」。
//!
//! 本文（コード・トークン・verifier）はログに出さず、エラーの説明にも入れない。

use axum::{
    Json, Router,
    extract::{Form, State, rejection::FormRejection},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL, WWW_AUTHENTICATE},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use common::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use service::desktop_auth::s256_challenge;
use service::oauth_server::client::client_is_allowed;
use service::oauth_server::grant::{self, CodeState, GrantError, IssuedTokens};
use service::oauth_server::{
    GRANTABLE_SCOPES, OFFLINE_ACCESS, REFRESH_TOKEN_PREFIX, issuer, resource_matches, resource_uri,
    scope_string,
};

use crate::AppState;
use crate::handlers::personal_tokens::ensure_can_bind_tenants;

pub(super) fn routes(state: AppState) -> Router {
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource/api/mcp",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server_metadata),
        )
        .route("/oauth/token", post(token))
        .route("/oauth/revoke", post(revoke))
        .with_state(state)
}

fn grantable_scope_names() -> Vec<&'static str> {
    GRANTABLE_SCOPES
        .iter()
        .map(|scope| scope.as_str())
        .collect()
}

/// Protected Resource Metadata（RFC 9728）。`resource` は、利用者がクライアントに入れる URL と
/// 同じ値にする。`offline_access` は載せない。
async fn protected_resource_metadata(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "resource": resource_uri(&state.settings),
        "authorization_servers": [issuer(&state.settings)],
        "scopes_supported": grantable_scope_names(),
        "bearer_methods_supported": ["header"],
        "resource_name": "task",
    }))
}

/// Authorization Server Metadata（RFC 8414）。`registration_endpoint` は出さない
/// （クライアントの登録は CIMD だけ）。
async fn authorization_server_metadata(State(state): State<AppState>) -> Json<Value> {
    let origin = issuer(&state.settings);
    let mut scopes = grantable_scope_names();
    scopes.push(OFFLINE_ACCESS);
    Json(json!({
        "issuer": origin,
        "authorization_endpoint": format!("{origin}/oauth/authorize"),
        "token_endpoint": format!("{origin}/api/oauth/token"),
        "revocation_endpoint": format!("{origin}/api/oauth/revoke"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": scopes,
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
    }))
}

/// トークンと失効のエンドポイントのエラー（RFC 6749 の形）。API 共通の `{"message": …}` とは別物。
#[derive(Debug)]
struct TokenError {
    status: StatusCode,
    error: &'static str,
    description: &'static str,
    /// `invalid_client` で、要求の `Authorization` と同じ方式を返すための値
    challenge: Option<HeaderValue>,
}

impl TokenError {
    fn new(status: StatusCode, error: &'static str, description: &'static str) -> Self {
        Self {
            status,
            error,
            description,
            challenge: None,
        }
    }

    fn invalid_request(description: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", description)
    }

    fn invalid_grant() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "The grant is invalid, expired, revoked, or was issued to another client",
        )
    }

    fn invalid_target() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "The resource is not served by this authorization server",
        )
    }

    fn invalid_client(challenge: Option<HeaderValue>) -> Self {
        Self {
            challenge,
            ..Self::new(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "Only public clients are accepted; do not send client authentication",
            )
        }
    }

    /// 原因はログに残し、応答には固定の文だけを返す。
    fn server_error(error: impl std::fmt::Debug) -> Self {
        tracing::error!(?error, "oauth token endpoint failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "The request could not be processed",
        )
    }
}

impl IntoResponse for TokenError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
            Json(json!({ "error": self.error, "error_description": self.description })),
        )
            .into_response();
        if let Some(challenge) = self.challenge {
            response.headers_mut().insert(WWW_AUTHENTICATE, challenge);
        }
        response
    }
}

impl From<GrantError> for TokenError {
    fn from(error: GrantError) -> Self {
        match error {
            GrantError::InvalidGrant => Self::invalid_grant(),
            GrantError::Db(error) => Self::server_error(error),
            GrantError::Internal(error) => Self::server_error(error),
        }
    }
}

/// 要求の `Authorization` の方式（`Basic` など）を、`WWW-Authenticate` の値にする。
fn challenge_for(authorization: &HeaderValue) -> Option<HeaderValue> {
    let scheme = authorization.to_str().ok()?.split(' ').next()?;
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    HeaderValue::from_str(scheme).ok()
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: Option<String>,
    code: Option<String>,
    code_verifier: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    resource: Option<String>,
    refresh_token: Option<String>,
    client_secret: Option<String>,
    client_assertion: Option<String>,
    client_assertion_type: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    scope: String,
}

impl From<IssuedTokens> for TokenResponse {
    fn from(tokens: IssuedTokens) -> Self {
        Self {
            access_token: tokens.access_token,
            token_type: "Bearer",
            expires_in: tokens.expires_in,
            refresh_token: tokens.refresh_token,
            // 同意で scope を絞れるので、常に返す
            scope: scope_string(&tokens.scopes),
        }
    }
}

fn required(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).filter(|value| !value.is_empty())
}

async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    form: Result<Form<TokenForm>, FormRejection>,
) -> Result<Response, TokenError> {
    // 受けるのは公開クライアントだけなので、クライアントの認証が付いた要求は断る。
    // `grant_type` の分岐より前に見る（コードを消費しない）
    if let Some(authorization) = headers.get(AUTHORIZATION) {
        return Err(TokenError::invalid_client(challenge_for(authorization)));
    }
    let Form(form) = form.map_err(|_| {
        TokenError::invalid_request("The body must be application/x-www-form-urlencoded")
    })?;
    if form.client_secret.is_some()
        || form.client_assertion.is_some()
        || form.client_assertion_type.is_some()
    {
        return Err(TokenError::invalid_client(None));
    }
    let tokens = match form.grant_type.as_deref() {
        Some("authorization_code") => exchange_code(&state, &form).await?,
        Some("refresh_token") => refresh(&state, &form).await?,
        _ => {
            return Err(TokenError::new(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "grant_type must be authorization_code or refresh_token",
            ));
        }
    };
    Ok((
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(TokenResponse::from(tokens)),
    )
        .into_response())
}

fn ensure_resource(state: &AppState, form: &TokenForm) -> Result<(), TokenError> {
    match form.resource.as_deref() {
        Some(resource) if !resource_matches(&state.settings, resource) => {
            Err(TokenError::invalid_target())
        }
        _ => Ok(()),
    }
}

/// 認可コードをトークンの組に替える。
///
/// コードは最初に消費するので、以後のどの失敗でも二度と使えない。同じコードがもう一度来たら、
/// そのコードから作った許可を取り消す。2 つの交換がどの順で進んでも、許可は残らない
/// （後から来た側が印を置いてから取り消し、先に来た側は commit のあとで印を読む）。
async fn exchange_code(state: &AppState, form: &TokenForm) -> Result<IssuedTokens, TokenError> {
    let (Some(code), Some(code_verifier), Some(client_id)) = (
        required(form.code.as_ref()),
        required(form.code_verifier.as_ref()),
        required(form.client_id.as_ref()),
    ) else {
        return Err(TokenError::invalid_request(
            "code, code_verifier and client_id are required",
        ));
    };
    ensure_resource(state, form)?;

    let pending = match grant::take_code(&state.redis_client, code)
        .await
        .map_err(TokenError::server_error)?
    {
        CodeState::Fresh(pending) => pending,
        CodeState::Used(grant_id) => {
            grant::revoke_grant(&state.db, grant_id)
                .await
                .map_err(TokenError::server_error)?;
            return Err(TokenError::invalid_grant());
        }
        CodeState::Unknown => return Err(TokenError::invalid_grant()),
    };
    if pending.client_id != client_id
        || form
            .redirect_uri
            .as_deref()
            .is_some_and(|redirect_uri| redirect_uri != pending.redirect_uri)
        || s256_challenge(code_verifier) != pending.code_challenge
    {
        return Err(TokenError::invalid_grant());
    }
    // 承認から交換までの間に、許可リストや所属が変わっていれば止める
    if !client_is_allowed(&state.settings, client_id) {
        return Err(TokenError::invalid_grant());
    }
    match ensure_can_bind_tenants(state, pending.user_id, &pending.tenant_ids).await {
        Ok(_) => {}
        Err(AppError::Forbidden | AppError::ForbiddenDetail(_) | AppError::NotFound) => {
            return Err(TokenError::invalid_grant());
        }
        Err(error) => return Err(TokenError::server_error(error)),
    }

    let tokens =
        grant::create_grant(&state.db, &state.settings.personal_token_secret, &pending).await?;

    match grant::code_was_replayed(&state.redis_client, code).await {
        Ok(false) => Ok(tokens),
        Ok(true) => {
            grant::revoke_grant(&state.db, pending.grant_id)
                .await
                .map_err(TokenError::server_error)?;
            Err(TokenError::invalid_grant())
        }
        // 確かめられないまま有効な許可を残さない
        Err(error) => {
            grant::revoke_grant(&state.db, pending.grant_id)
                .await
                .map_err(TokenError::server_error)?;
            Err(TokenError::server_error(error))
        }
    }
}

/// リフレッシュトークンを回す。要求の `scope` は読まない（発行する scope は許可の行に在る）。
async fn refresh(state: &AppState, form: &TokenForm) -> Result<IssuedTokens, TokenError> {
    let (Some(refresh_token), Some(client_id)) = (
        required(form.refresh_token.as_ref()),
        required(form.client_id.as_ref()),
    ) else {
        return Err(TokenError::invalid_request(
            "refresh_token and client_id are required",
        ));
    };
    ensure_resource(state, form)?;
    // 形の違う値は DB を引かずに断る
    if !refresh_token.starts_with(REFRESH_TOKEN_PREFIX) {
        return Err(TokenError::invalid_grant());
    }
    Ok(grant::rotate_refresh_token(&state.db, &state.settings, refresh_token, client_id).await?)
}

#[derive(Deserialize)]
struct RevokeForm {
    token: Option<String>,
    client_id: Option<String>,
}

/// トークンの失効（RFC 7009）。アクセストークンでもリフレッシュトークンでも、その許可ごと取り消す。
/// 未知のトークンでも 200 を返す。
async fn revoke(
    State(state): State<AppState>,
    form: Result<Form<RevokeForm>, FormRejection>,
) -> Result<Response, TokenError> {
    let Form(form) = form.map_err(|_| {
        TokenError::invalid_request("The body must be application/x-www-form-urlencoded")
    })?;
    let Some(token) = required(form.token.as_ref()) else {
        return Err(TokenError::invalid_request("token is required"));
    };
    grant::revoke_by_token(
        &state.db,
        &state.settings.personal_token_secret,
        token,
        required(form.client_id.as_ref()),
    )
    .await?;
    Ok((
        StatusCode::OK,
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_json(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn token_error_uses_the_rfc_6749_shape() {
        let response = TokenError::invalid_grant().into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        assert!(response.headers().get(WWW_AUTHENTICATE).is_none());
        let body = body_json(response).await;
        assert_eq!(body["error"], "invalid_grant");
        assert!(body["error_description"].is_string());
        assert!(body.get("message").is_none());
    }

    #[tokio::test]
    async fn invalid_client_is_401_and_echoes_the_scheme() {
        let challenge = challenge_for(&HeaderValue::from_static("Basic dXNlcjpwYXNz"));
        let response = TokenError::invalid_client(challenge).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[WWW_AUTHENTICATE], "Basic");
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(body_json(response).await["error"], "invalid_client");

        let without = TokenError::invalid_client(None).into_response();
        assert!(without.headers().get(WWW_AUTHENTICATE).is_none());
    }

    #[test]
    fn challenge_takes_only_a_plain_scheme() {
        let scheme = |raw: &'static str| {
            challenge_for(&HeaderValue::from_static(raw))
                .map(|value| value.to_str().unwrap().to_string())
        };
        assert_eq!(scheme("Basic abc"), Some("Basic".into()));
        assert_eq!(scheme("Bearer abc"), Some("Bearer".into()));
        assert_eq!(scheme("Basic"), Some("Basic".into()));
        assert_eq!(scheme(""), None);
        assert_eq!(scheme(" abc"), None);
        assert_eq!(scheme("Ba\"sic abc"), None);
    }

    #[tokio::test]
    async fn server_error_does_not_leak_the_cause() {
        let response = TokenError::server_error("redis is down: secret").into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_json(response).await;
        assert_eq!(body["error"], "server_error");
        assert!(!body.to_string().contains("secret"));
    }

    #[test]
    fn token_response_omits_a_missing_refresh_token() {
        let tokens = |refresh_token: Option<String>| IssuedTokens {
            access_token: "mcpat_x".into(),
            refresh_token,
            expires_in: 3600,
            scopes: vec![entity::scopes::Scope::ReadTask],
        };
        let with = serde_json::to_value(TokenResponse::from(tokens(Some("mcprt_x".into()))))
            .expect("json");
        assert_eq!(with["refresh_token"], "mcprt_x");
        assert_eq!(with["token_type"], "Bearer");
        assert_eq!(with["scope"], "read:task");
        let without = serde_json::to_value(TokenResponse::from(tokens(None))).expect("json");
        assert!(without.get("refresh_token").is_none());
    }
}

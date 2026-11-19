//! 再認証（直近の本人確認）。
//!
//! パスキーの追加と削除の前に、直近の本人確認を求める（`service::reauth`）。
//! 手段は、パスワード、既存のパスキー、既に結んだ OAuth の連携。連携は `GET /v1/auth/oauth/{provider}`
//! の `reauth=true` で行う（`handlers/oauth.rs`）。

use axum::{Json, extract::State, http::StatusCode};
use axum_session::Session;
use axum_session_redispool::SessionRedisPool;
use axum_valid::Valid;
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use webauthn_rs::prelude::{PublicKeyCredential, WebauthnError};

use crate::AppState;
use crate::extractors::CurrentUser;
use crate::handlers::passkeys::finish_passkey_authentication;
use crate::openapi::SessionAuthErrors;
use entity::oauth_connections;
use payload::reauth::*;
use service::auth::{AuthError, verify_password};
use service::passkey_challenges;
use service::passkeys::{
    client_data_is_same_origin, count_user_passkeys, find_by_credential_id, load_user_passkeys,
};
use service::reauth::{REAUTH_SESSION_KEY, fresh_until, mark_reauthed, remaining_secs};

type AppSession = Session<SessionRedisPool>;

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/reauth",
    tag = "Auth",
    summary = "再認証の要否と手段",
    description = "直近の本人確認が有効期間の内か（内なら切れる時刻）と、再認証に使える手段を返す。画面は、再認証を求める操作（連携の追加・初回のパスワードの設定・パスキーの追加と削除）の前にこれを引き、要れば先に本人確認を求める。",
    responses(
        (status = 200, description = "再認証の要否と手段", body = ReauthStatusResponse),
        SessionAuthErrors,
    )
)]
pub async fn reauth_status(
    session: AppSession,
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<ReauthStatusResponse>, AuthError> {
    let now_ms = Utc::now().timestamp_millis();
    let reauthed_at = session.get::<i64>(REAUTH_SESSION_KEY);
    let window = state.settings.reauth_window_secs;
    let expires_at = fresh_until(reauthed_at, now_ms, window)
        .and_then(DateTime::from_timestamp_millis)
        .map(|at| at.to_rfc3339());
    let remaining_secs = remaining_secs(reauthed_at, now_ms, window);
    let passkey = count_user_passkeys(&state.db, user.id).await? > 0;
    let connections = oauth_connections::Entity::find()
        .filter(oauth_connections::Column::UserId.eq(user.id))
        .all(&state.db)
        .await?;
    let oauth = connections
        .into_iter()
        .filter_map(|row| {
            let slug = state
                .oauth_settings
                .reauth_provider_slug(&row.provider, row.instance_url.as_deref())?;
            Some(ReauthOAuthMethod {
                provider: slug.to_string(),
                connection_provider: row.provider,
                instance_url: row.instance_url,
            })
        })
        .collect();
    Ok(Json(ReauthStatusResponse {
        fresh: expires_at.is_some(),
        expires_at,
        remaining_secs,
        window_secs: state.settings.reauth_window_secs,
        methods: ReauthMethods {
            password: user.password_hash.is_some(),
            passkey,
            oauth,
        },
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/reauth/password",
    tag = "Auth",
    summary = "パスワードで再認証",
    description = "パスキーの追加と削除の前に求める本人確認を、パスワードで行う。成功すると、有効期間（既定 300 秒）の内は再認証を求めない。パスワードを持たない利用者は 400（`password-not-set`）。",
    request_body = ReauthPasswordRequest,
    responses(
        (status = 204, description = "再認証に成功"),
        SessionAuthErrors,
    )
)]
pub async fn reauth_password(
    session: AppSession,
    State(_state): State<AppState>,
    user: CurrentUser,
    Valid(Json(payload)): Valid<Json<ReauthPasswordRequest>>,
) -> Result<StatusCode, AuthError> {
    let hash = user
        .password_hash
        .as_deref()
        .ok_or(AuthError::PasswordNotSet)?;
    if !verify_password(&payload.password, hash)? {
        return Err(AuthError::InvalidCurrentPassword);
    }
    mark_reauthed(&session);
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/reauth/passkey/start",
    tag = "Auth",
    summary = "パスキーで再認証するチャレンジ発行",
    description = "自分のパスキーに限った PublicKeyCredentialRequestOptions を返す。パスキーが無ければ 404。",
    responses(
        (status = 200, description = "PublicKeyCredentialRequestOptions (WebAuthn JSON)"),
        SessionAuthErrors,
    )
)]
pub async fn reauth_passkey_start(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<serde_json::Value>, AuthError> {
    let passkeys = load_user_passkeys(&state.db, user.id)
        .await
        .map_err(AuthError::Internal)?;
    if passkeys.is_empty() {
        return Err(AuthError::PasskeyNotFound);
    }
    let (rcr, auth_state) = state.webauthn.start_passkey_authentication(&passkeys)?;
    passkey_challenges::store_reauth(&state.redis_client, user.id, &auth_state)
        .await
        .map_err(AuthError::Internal)?;
    Ok(Json(
        serde_json::to_value(&rcr).map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?,
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/reauth/passkey/finish",
    tag = "Auth",
    summary = "パスキーで再認証",
    description = "自分のパスキーの署名を検めて、本人確認の印を立てる。他人のパスキーは 401。",
    request_body = ReauthPasskeyFinishRequest,
    responses(
        (status = 204, description = "再認証に成功"),
        SessionAuthErrors,
    )
)]
pub async fn reauth_passkey_finish(
    session: AppSession,
    State(state): State<AppState>,
    user: CurrentUser,
    Json(payload): Json<ReauthPasskeyFinishRequest>,
) -> Result<StatusCode, AuthError> {
    let credential: PublicKeyCredential =
        serde_json::from_value(payload.credential).map_err(|_| AuthError::BadRequest)?;
    if !client_data_is_same_origin(&credential.response.client_data_json) {
        return Err(WebauthnError::CredentialCrossOrigin.into());
    }

    let auth_state = passkey_challenges::take_reauth(&state.redis_client, user.id)
        .await
        .map_err(AuthError::Internal)?
        .ok_or(AuthError::BadRequest)?;

    // 自分のパスキーだけを受ける（他人のパスキーの credential ID は 401）
    let stored = find_by_credential_id(&state.db, credential.get_credential_id())
        .await?
        .filter(|row| row.user_id == user.id)
        .ok_or(AuthError::InvalidCredentials)?;

    finish_passkey_authentication(&state, &credential, auth_state, stored).await?;
    mark_reauthed(&session);
    Ok(StatusCode::NO_CONTENT)
}

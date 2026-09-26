//! Koyori Desktop の認証（Authorization Code + PKCE、loopback）と端末管理。
//! 規則は apps/backend/docs/personal-access-tokens-authz.md の「Desktop 認証」。
//! code / token / code_verifier はログに出さない。

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use axum_valid::Valid;
use chrono::Utc;
use sea_orm::prelude::Uuid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::AppState;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::{CrudErrors, DesktopAuthTokenErrors, SessionAuthErrors};
use entity::{device_tokens, users};
use payload::desktop_auth::*;
use service::auth::AuthError;
use service::desktop_auth::{self, PendingCode};

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/auth/codes",
    tag = "Desktop",
    summary = "Desktop 用の認可コードを発行",
    description = "Web の承認画面から呼ぶ。セッション専用（Bearer・2FA 途中のセッションは 403）。コードは 5 分・一度きり。",
    request_body = DesktopAuthCodeRequest,
    responses(
        (status = 201, description = "認可コード", body = DesktopAuthCodeResponse),
        SessionAuthErrors,
    )
)]
pub async fn create_desktop_auth_code(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(payload)): Valid<Json<DesktopAuthCodeRequest>>,
) -> Result<(StatusCode, Json<DesktopAuthCodeResponse>), AuthError> {
    auth.require_session().map_err(|_| AuthError::Forbidden)?;
    let code = desktop_auth::issue_code(
        &state.redis_client,
        &PendingCode {
            user_id: auth.user_id,
            code_challenge: payload.code_challenge,
            name: payload.name,
            issued_at_ms: Utc::now().timestamp_millis(),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(DesktopAuthCodeResponse { code })))
}

/// レート制限の接続元キー。プロキシが付ける値を使う（監査ログと同じ取り方）。
fn client_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/auth/token",
    tag = "Desktop",
    summary = "認可コードを Device Token に交換",
    description = "未認証で呼ぶ（code が資格）。code の不一致・期限切れ・再利用・code_verifier の不一致はすべて 401 で、code は消費済みになる。平文トークンはこの応答でのみ返す。",
    request_body = DesktopAuthTokenRequest,
    responses(
        (status = 201, description = "発行した Device Token", body = DesktopAuthTokenResponse),
        DesktopAuthTokenErrors,
    )
)]
pub async fn exchange_desktop_auth_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Valid(Json(payload)): Valid<Json<DesktopAuthTokenRequest>>,
) -> Result<(StatusCode, Json<DesktopAuthTokenResponse>), AuthError> {
    if !desktop_auth::try_acquire_token_attempt(&state.redis_client, &client_key(&headers)).await? {
        return Err(AuthError::TooManyRequests);
    }
    // GETDEL で先に消費するので、verifier の不一致でも code は二度と使えない
    let pending = desktop_auth::take_code(&state.redis_client, &payload.code)
        .await?
        .ok_or(AuthError::Unauthorized)?;
    if desktop_auth::s256_challenge(&payload.code_verifier) != pending.code_challenge {
        return Err(AuthError::Unauthorized);
    }
    let authorized_at = chrono::DateTime::from_timestamp_millis(pending.issued_at_ms)
        .filter(|_| pending.issued_at_ms > 0)
        .ok_or(AuthError::Unauthorized)?;
    let user = users::Entity::find_by_id(pending.user_id)
        .one(&state.db)
        .await?
        .ok_or(AuthError::Unauthorized)?;
    if user.is_suspended
        || user
            .sessions_revoked_at
            .is_some_and(|revoked_at| revoked_at.timestamp_millis() >= pending.issued_at_ms)
    {
        return Err(AuthError::Unauthorized);
    }

    let (token, model) = desktop_auth::create_device_token(
        &state.db,
        &state.settings.personal_token_secret,
        pending.user_id,
        pending.name,
        authorized_at,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(DesktopAuthTokenResponse {
            token,
            expires_at: model.expires_at.with_timezone(&Utc),
            device_id: model.id,
        }),
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/me/devices",
    tag = "Desktop",
    summary = "自分の端末（Device Token）の一覧",
    description = "失効済み・期限切れを除く。セッションと Device Token で呼べる。",
    responses(
        (status = 200, description = "端末の一覧（新しい順）", body = Vec<DeviceToken>),
        SessionAuthErrors,
    )
)]
pub async fn list_devices(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<DeviceToken>>, AppError> {
    auth.require_session_or_device_token()?;
    let rows = device_tokens::Entity::find()
        .filter(device_tokens::Column::UserId.eq(auth.user_id))
        .filter(device_tokens::Column::RevokedAt.is_null())
        .filter(device_tokens::Column::ExpiresAt.gt(Utc::now()))
        .order_by_desc(device_tokens::Column::CreatedAt)
        .order_by_desc(device_tokens::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(DeviceToken::from).collect()))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/me/devices/{id}",
    tag = "Desktop",
    summary = "端末を失効",
    description = "revoked_at を立てる（行は残す）。要求に使っている Device Token 自身も失効できる（ログアウト）。他人の端末は 404。",
    params(("id" = Uuid, Path, description = "端末の識別子")),
    responses(
        (status = 204, description = "失効しました"),
        CrudErrors,
    )
)]
pub async fn revoke_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    auth.require_session_or_device_token()?;
    let row = device_tokens::Entity::find_by_id(id)
        .filter(device_tokens::Column::UserId.eq(auth.user_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.revoked_at.is_none() {
        let mut active: device_tokens::ActiveModel = row.into();
        active.revoked_at = Set(Some(Utc::now().into()));
        active.update(&state.db).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

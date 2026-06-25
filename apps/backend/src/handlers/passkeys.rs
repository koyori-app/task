use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_session::Session;
use axum_session_redispool::SessionRedisPool;
use axum_valid::Valid;
use chrono::Utc;
use sea_orm::prelude::Uuid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use serde::Serialize;
use webauthn_rs::prelude::{
    CredentialID, DiscoverableKey, Passkey, PublicKeyCredential, RegisterPublicKeyCredential,
};

use crate::AppState;
use crate::entities::{passkeys as passkey_entity, users};
use crate::extractors::CurrentUser;
use crate::openapi::SessionAuthErrors;
use crate::payload::passkeys::*;
use crate::utils::auth::AuthError;
use crate::utils::email::normalize_email;
use crate::utils::passkey_challenges;
use crate::utils::passkeys::{
    MAX_PASSKEYS_PER_USER, count_user_passkeys, find_by_credential_id,
    insert_passkey_under_user_lock, is_last_auth_method, load_user_passkeys, model_to_passkey,
    passkey_to_model_fields, update_passkey_after_authentication,
};

type AppSession = Session<SessionRedisPool>;

fn to_list_item(model: passkey_entity::Model) -> PasskeyListItem {
    PasskeyListItem {
        id: model.id,
        name: model.name,
        last_used_at: model.last_used_at.map(|t| t.to_rfc3339()),
        created_at: model.created_at.to_rfc3339(),
    }
}

fn exclude_credentials(passkeys: &[Passkey]) -> Option<Vec<CredentialID>> {
    if passkeys.is_empty() {
        None
    } else {
        Some(passkeys.iter().map(|p| p.cred_id().clone()).collect())
    }
}

fn challenge_response(
    rcr: impl Serialize,
    challenge_id: Uuid,
) -> Result<Json<serde_json::Value>, AuthError> {
    let mut value =
        serde_json::to_value(&rcr).map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "challenge_id".to_string(),
            serde_json::Value::String(challenge_id.to_string()),
        );
    }
    Ok(Json(value))
}

/// メール指定時: ユーザー不在・未確認・パスキー無しでも同一形状のダミーチャレンジを返す。
async fn passkeys_for_email_auth(
    db: &sea_orm::DatabaseConnection,
    email: &str,
) -> Result<Vec<Passkey>, AuthError> {
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(email))
        .one(db)
        .await?;

    let Some(user) = user.filter(|u| u.email_verified) else {
        return Ok(vec![]);
    };

    let passkeys = load_user_passkeys(db, user.id)
        .await
        .map_err(AuthError::Internal)?;
    Ok(passkeys)
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/passkeys/registration/start",
    tag = "Auth",
    summary = "パスキー登録チャレンジ発行",
    responses(
        (status = 200, description = "PublicKeyCredentialCreationOptions (WebAuthn JSON)"),
        SessionAuthErrors,
    )
)]
pub async fn registration_start(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<serde_json::Value>, AuthError> {
    user.0
        .email_verified
        .then_some(())
        .ok_or(AuthError::Forbidden)?;

    let lock_acquired = passkey_challenges::acquire_registration_lock(&state.redis_client, user.id)
        .await
        .map_err(AuthError::Internal)?;
    if !lock_acquired {
        return Err(AuthError::RegistrationInProgress);
    }

    let count = match count_user_passkeys(&state.db, user.id).await {
        Ok(count) => count,
        Err(e) => {
            let _ =
                passkey_challenges::release_registration_lock(&state.redis_client, user.id).await;
            return Err(e.into());
        }
    };
    if count >= MAX_PASSKEYS_PER_USER {
        let _ = passkey_challenges::release_registration_lock(&state.redis_client, user.id).await;
        return Err(AuthError::PasskeyLimitExceeded);
    }

    let existing = match load_user_passkeys(&state.db, user.id).await {
        Ok(passkeys) => passkeys,
        Err(e) => {
            let _ =
                passkey_challenges::release_registration_lock(&state.redis_client, user.id).await;
            return Err(AuthError::Internal(e));
        }
    };
    let exclude = exclude_credentials(&existing);

    let (ccr, reg_state) = match state.webauthn.start_passkey_registration(
        user.id,
        &user.email,
        &user.username,
        exclude,
    ) {
        Ok(v) => v,
        Err(e) => {
            let _ =
                passkey_challenges::release_registration_lock(&state.redis_client, user.id).await;
            return Err(e.into());
        }
    };

    if let Err(e) =
        passkey_challenges::store_registration(&state.redis_client, user.id, &reg_state).await
    {
        let _ = passkey_challenges::release_registration_lock(&state.redis_client, user.id).await;
        return Err(AuthError::Internal(e));
    }

    Ok(Json(
        serde_json::to_value(&ccr).map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?,
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/passkeys/registration/finish",
    tag = "Auth",
    summary = "パスキー登録完了",
    request_body = PasskeyRegistrationFinishRequest,
    responses(
        (status = 201, description = "登録完了"),
        SessionAuthErrors,
    )
)]
pub async fn registration_finish(
    State(state): State<AppState>,
    user: CurrentUser,
    Valid(Json(payload)): Valid<Json<PasskeyRegistrationFinishRequest>>,
) -> Result<StatusCode, AuthError> {
    user.0
        .email_verified
        .then_some(())
        .ok_or(AuthError::Forbidden)?;

    if !passkey_challenges::registration_lock_held(&state.redis_client, user.id)
        .await
        .map_err(AuthError::Internal)?
    {
        return Err(AuthError::BadRequest);
    }

    let release_lock = || async {
        let _ = passkey_challenges::release_registration_lock(&state.redis_client, user.id).await;
    };

    let reg_state = match passkey_challenges::take_registration(&state.redis_client, user.id).await
    {
        Ok(Some(s)) => s,
        Ok(None) => {
            release_lock().await;
            return Err(AuthError::BadRequest);
        }
        Err(e) => {
            release_lock().await;
            return Err(AuthError::Internal(e));
        }
    };

    let credential: RegisterPublicKeyCredential = match serde_json::from_value(payload.credential) {
        Ok(c) => c,
        Err(_) => {
            release_lock().await;
            return Err(AuthError::BadRequest);
        }
    };
    let passkey = match state
        .webauthn
        .finish_passkey_registration(&credential, &reg_state)
    {
        Ok(p) => p,
        Err(e) => {
            release_lock().await;
            return Err(e.into());
        }
    };

    let (credential_id, public_key, aaguid, sign_count) = match passkey_to_model_fields(&passkey) {
        Ok(f) => f,
        Err(e) => {
            release_lock().await;
            return Err(AuthError::Internal(e));
        }
    };
    let now = Utc::now();

    let insert_result = insert_passkey_under_user_lock(
        &state.db,
        user.id,
        passkey_entity::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user.id),
            credential_id: Set(credential_id),
            public_key: Set(public_key),
            aaguid: Set(aaguid),
            sign_count: Set(sign_count),
            name: Set(payload.name),
            last_used_at: Set(None),
            created_at: Set(now),
        },
    )
    .await;

    release_lock().await;

    match insert_result {
        Ok(()) => Ok(StatusCode::CREATED),
        Err(e) => Err(e),
    }
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/passkeys/authentication/start",
    tag = "Auth",
    summary = "パスキー認証チャレンジ発行",
    request_body = PasskeyAuthenticationStartRequest,
    responses(
        (status = 200, description = "PublicKeyCredentialRequestOptions (WebAuthn JSON) + challenge_id"),
        SessionAuthErrors,
    )
)]
pub async fn authentication_start(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<PasskeyAuthenticationStartRequest>>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let challenge_id = Uuid::new_v4();

    if let Some(email) = payload.email.as_deref() {
        let email = normalize_email(email);
        let passkey_list = passkeys_for_email_auth(&state.db, &email).await?;

        // パスキーが存在する場合のみ allowCredentials 付きチャレンジを返す。
        // 空リストで start_passkey_authentication すると空の allowCredentials が返り、
        // 攻撃者がパスキー未登録アカウントを識別できるため、その場合は discoverable に fallback する。
        if !passkey_list.is_empty() {
            let (rcr, auth_state) = state.webauthn.start_passkey_authentication(&passkey_list)?;

            passkey_challenges::store_authentication(
                &state.redis_client,
                challenge_id,
                &auth_state,
            )
            .await
            .map_err(AuthError::Internal)?;

            return challenge_response(rcr, challenge_id);
        }
    }

    let (rcr, auth_state) = state.webauthn.start_discoverable_authentication()?;

    passkey_challenges::store_discoverable_authentication(
        &state.redis_client,
        challenge_id,
        &auth_state,
    )
    .await
    .map_err(AuthError::Internal)?;

    challenge_response(rcr, challenge_id)
}

async fn finish_passkey_authentication(
    state: &AppState,
    credential: &PublicKeyCredential,
    auth_state: webauthn_rs::prelude::PasskeyAuthentication,
    stored: passkey_entity::Model,
) -> Result<(), AuthError> {
    let mut passkey = model_to_passkey(&stored).map_err(AuthError::Internal)?;
    let auth_result = state
        .webauthn
        .finish_passkey_authentication(credential, &auth_state)?;

    update_passkey_after_authentication(&state.db, stored, &mut passkey, &auth_result).await
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/passkeys/authentication/finish",
    tag = "Auth",
    summary = "パスキー認証完了・セッション発行",
    request_body = PasskeyAuthenticationFinishRequest,
    responses(
        (status = 204, description = "認証成功（2FA スキップ）"),
        SessionAuthErrors,
    )
)]
pub async fn authentication_finish(
    session: AppSession,
    State(state): State<AppState>,
    Json(payload): Json<PasskeyAuthenticationFinishRequest>,
) -> Result<StatusCode, AuthError> {
    let credential: PublicKeyCredential =
        serde_json::from_value(payload.credential).map_err(|_| AuthError::BadRequest)?;

    if let Some(auth_state) = passkey_challenges::take_discoverable_authentication(
        &state.redis_client,
        payload.challenge_id,
    )
    .await
    .map_err(AuthError::Internal)?
    {
        let (user_id, cred_id) = state
            .webauthn
            .identify_discoverable_authentication(&credential)
            .map_err(|_| AuthError::InvalidCredentials)?;

        let stored = find_by_credential_id(&state.db, cred_id)
            .await?
            .filter(|row| row.user_id == user_id)
            .ok_or(AuthError::InvalidCredentials)?;

        let user = users::Entity::find_by_id(stored.user_id)
            .one(&state.db)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        if !user.email_verified {
            return Err(AuthError::InvalidCredentials);
        }

        if user.is_suspended {
            return Err(AuthError::Suspended);
        }

        let mut passkey = model_to_passkey(&stored).map_err(AuthError::Internal)?;
        let auth_result = state.webauthn.finish_discoverable_authentication(
            &credential,
            auth_state,
            &[DiscoverableKey::from(&passkey)],
        )?;

        update_passkey_after_authentication(&state.db, stored, &mut passkey, &auth_result).await?;

        session.renew();
        session.set("issued_at_ms", Utc::now().timestamp_millis());
        session.set("user_id", user.id);
        return Ok(StatusCode::NO_CONTENT);
    }

    let auth_state =
        passkey_challenges::take_authentication(&state.redis_client, payload.challenge_id)
            .await
            .map_err(AuthError::Internal)?
            .ok_or(AuthError::BadRequest)?;

    let cred_id = credential.get_credential_id();
    let stored = find_by_credential_id(&state.db, cred_id)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let user = users::Entity::find_by_id(stored.user_id)
        .one(&state.db)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    if !user.email_verified {
        return Err(AuthError::InvalidCredentials);
    }

    if user.is_suspended {
        return Err(AuthError::Suspended);
    }

    finish_passkey_authentication(&state, &credential, auth_state, stored).await?;

    session.renew();
    session.set("issued_at_ms", Utc::now().timestamp_millis());
    session.set("user_id", user.id);
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/passkeys",
    tag = "Auth",
    summary = "登録済みパスキー一覧",
    responses(
        (status = 200, description = "パスキー一覧", body = PasskeyListResponse),
        SessionAuthErrors,
    )
)]
pub async fn list_passkeys(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<PasskeyListResponse>, AuthError> {
    let rows = passkey_entity::Entity::find()
        .filter(passkey_entity::Column::UserId.eq(user.id))
        .all(&state.db)
        .await?;

    Ok(Json(PasskeyListResponse {
        passkeys: rows.into_iter().map(to_list_item).collect(),
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path = "/passkeys/{id}",
    tag = "Auth",
    summary = "パスキー名変更",
    request_body = PasskeyRenameRequest,
    responses(
        (status = 204, description = "更新完了"),
        SessionAuthErrors,
    )
)]
pub async fn rename_passkey(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<PasskeyRenameRequest>>,
) -> Result<StatusCode, AuthError> {
    let row = passkey_entity::Entity::find_by_id(id)
        .filter(passkey_entity::Column::UserId.eq(user.id))
        .one(&state.db)
        .await?
        .ok_or(AuthError::PasskeyNotFound)?;

    let mut active: passkey_entity::ActiveModel = row.into();
    active.name = Set(payload.name);
    active.update(&state.db).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/passkeys/{id}",
    tag = "Auth",
    summary = "パスキー削除",
    responses(
        (status = 204, description = "削除完了"),
        SessionAuthErrors,
    )
)]
pub async fn delete_passkey(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AuthError> {
    state
        .db
        .transaction::<_, (), AuthError>(|txn| {
            Box::pin(async move {
                let row = passkey_entity::Entity::find_by_id(id)
                    .filter(passkey_entity::Column::UserId.eq(user.id))
                    .one(txn)
                    .await?
                    .ok_or(AuthError::PasskeyNotFound)?;

                // count と削除を同一トランザクション内に収め、並行削除による
                // 「全パスキー削除」競合を防止する。
                let count = count_user_passkeys(txn, user.id).await?;
                if is_last_auth_method(txn, user.id, count)
                    .await
                    .map_err(AuthError::Internal)?
                {
                    return Err(AuthError::LastAuthMethod);
                }

                passkey_entity::Entity::delete_by_id(row.id)
                    .exec(txn)
                    .await?;

                Ok(())
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(db_err) => AuthError::from(db_err),
            sea_orm::TransactionError::Transaction(auth_err) => auth_err,
        })?;

    Ok(StatusCode::NO_CONTENT)
}

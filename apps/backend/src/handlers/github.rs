use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use hmac::{Hmac, KeyInit, Mac};
use sea_orm::{
    prelude::DateTimeWithTimeZone, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait,
    QueryFilter,
};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::entities::{github_integrations, projects, tenants};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::jobs::github_webhook::{self, GithubWebhookJob};
use crate::openapi::CrudErrors;
use crate::settings::GithubAppSettings;
use crate::utils::{
    github_api, github_oauth_state::{self, GithubOAuthStatePayload},
    github_token_crypto,
};
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct GithubCallbackQuery {
    pub installation_id: i64,
    pub state: String,
    /// GitHub が送る操作種別。"request" はオーナー承認待ちであり連携未完了。
    #[serde(default)]
    pub setup_action: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GithubIntegrationResponse {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String, format = "date-time", nullable)]
    pub connected_at: Option<DateTimeWithTimeZone>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GithubInstallUrlResponse {
    pub url: String,
}

async fn require_tenant_owner(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id != user_id {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn require_project_in_tenant(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
) -> Result<(), AppError> {
    let exists = projects::Entity::find_by_id(project_id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

fn install_redirect_url(github: &GithubAppSettings, state: &str) -> String {
    format!(
        "https://github.com/apps/{}/installations/new?state={}",
        github.github_app_name, state
    )
}

fn settings_redirect_url(
    github: &GithubAppSettings,
    tenant_id: Uuid,
    project_id: Uuid,
) -> String {
    let base = github.github_app_frontend_base_url.trim_end_matches('/');
    format!("{base}/tenants/{tenant_id}/projects/{project_id}/settings/github")
}

/// GitHub Webhook 署名検証（HMAC-SHA256, ConstantTimeEq）。
pub fn verify_webhook_signature(secret: &str, signature_header: &str, body: &[u8]) -> bool {
    let Some(hex_digest) = signature_header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_digest) else {
        return false;
    };
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(body);
    let computed = mac.finalize().into_bytes();
    expected.ct_eq(computed.as_slice()).into()
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/install",
    tag = "GitHub",
    summary = "GitHub App インストール URL 取得",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, body = GithubInstallUrlResponse, description = "GitHub インストール URL"),
        CrudErrors,
    )
)]
pub async fn start_github_install(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<GithubInstallUrlResponse>, AppError> {
    let github = state.settings.require_github_app()?;
    auth.require_session()?;
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    require_project_in_tenant(&state, tenant_id, project_id).await?;

    let existing_installation_id = github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .map(|row| row.installation_id);

    let state_token = github_oauth_state::new_state_token();
    github_oauth_state::store_state(
        &state.redis_client,
        &state_token,
        &GithubOAuthStatePayload {
            tenant_id,
            project_id,
            user_id: auth.user_id,
            installation_id: existing_installation_id,
        },
    )
    .await
    .map_err(AppError::Internal)?;

    let url = install_redirect_url(github, &state_token);
    Ok(Json(GithubInstallUrlResponse { url }))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/callback",
    tag = "GitHub",
    summary = "GitHub App インストールコールバック",
    params(GithubCallbackQuery),
    responses(
        (status = 302, description = "設定画面へリダイレクト"),
        (status = 400, description = "無効な state / setup_action=request"),
        (status = 403, description = "ユーザー不一致"),
    )
)]
pub async fn github_callback(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<GithubCallbackQuery>,
) -> Result<Response, AppError> {
    let github = state.settings.require_github_app()?;
    auth.require_session()?;

    // setup_action=request はオーナーへの承認リクエスト段階。インストール未完了なので拒否。
    if query.setup_action.as_deref() == Some("request") {
        tracing::info!(
            installation_id = query.installation_id,
            "github callback: setup_action=request, installation pending owner approval"
        );
        return Err(AppError::BadRequest);
    }

    let payload = github_oauth_state::consume_state(&state.redis_client, &query.state)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::BadRequest)?;

    if payload.user_id != auth.user_id {
        return Err(AppError::Forbidden);
    }

    require_tenant_owner(&state, payload.tenant_id, auth.user_id).await?;
    require_project_in_tenant(&state, payload.tenant_id, payload.project_id).await?;

    let installation = github_api::verify_installation_for_callback(
        &state.http_client,
        github,
        query.installation_id,
        payload.installation_id,
    )
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "github callback installation verification failed");
        AppError::BadRequest
    })?;

    let access =
        github_api::fetch_installation_access_token(&state.http_client, github, installation.id)
            .await
            .map_err(AppError::Internal)?;
    let account_login = installation.account_login;
    let (repo_owner, repo_name) =
        github_api::fetch_primary_repository(&state.http_client, &access.token, &account_login)
            .await
            .map_err(AppError::Internal)?;

    let token_enc = github_token_crypto::encrypt_token(
        &github.github_token_encryption_key,
        &access.token,
    )
    .map_err(AppError::Internal)?;

    let now: DateTimeWithTimeZone = chrono::Utc::now().fixed_offset().into();
    let existing = github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(payload.project_id))
        .one(&state.db)
        .await?;

    if let Some(model) = existing {
        // 再連携: created_by / created_at は変更しない
        let mut active: github_integrations::ActiveModel = model.into();
        active.installation_id = Set(query.installation_id);
        active.repo_owner = Set(repo_owner);
        active.repo_name = Set(repo_name);
        active.access_token_enc = Set(token_enc);
        active.token_expires_at = Set(access.expires_at.into());
        active.update(&state.db).await?;
    } else {
        github_integrations::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(payload.project_id),
            installation_id: Set(query.installation_id),
            repo_owner: Set(repo_owner),
            repo_name: Set(repo_name),
            access_token_enc: Set(token_enc),
            token_expires_at: Set(access.expires_at.into()),
            created_by: Set(auth.user_id),
            created_at: Set(now),
        }
        .insert(&state.db)
        .await?;
    }

    let redirect_to = settings_redirect_url(github, payload.tenant_id, payload.project_id);
    Ok(Redirect::temporary(&redirect_to).into_response())
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/webhook",
    tag = "GitHub",
    summary = "GitHub Webhook 受信",
    responses(
        (status = 200, description = "受信成功"),
        (status = 403, description = "署名不一致"),
    )
)]
pub async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let github = state.settings.require_github_app()?;
    let signature = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Forbidden)?;
    if !verify_webhook_signature(&github.github_app_webhook_secret, signature, &body) {
        return Err(AppError::Forbidden);
    }

    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let delivery_id = headers
        .get("X-GitHub-Delivery")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| AppError::BadRequest)?;

    let installation_id = payload
        .get("installation")
        .and_then(|i| i.get("id"))
        .and_then(|id| id.as_i64())
        .or_else(|| payload.get("installation_id").and_then(|id| id.as_i64()));

    if let Some(installation_id) = installation_id {
        match github_integrations::Entity::find()
            .filter(github_integrations::Column::InstallationId.eq(installation_id))
            .one(&state.db)
            .await?
        {
            Some(integration) => {
                github_webhook::enqueue(
                    &state.github_webhook_storage,
                    GithubWebhookJob {
                        integration_id: integration.id,
                        project_id: integration.project_id,
                        event: event.clone(),
                        delivery_id: delivery_id.clone(),
                        payload: payload.clone(),
                    },
                )
                .await
                .map_err(AppError::Internal)?;
            }
            None => {
                tracing::warn!(
                    installation_id,
                    event = %event,
                    delivery_id = ?delivery_id,
                    "github webhook: no integration found for installation_id"
                );
            }
        }
    }

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/integration",
    tag = "GitHub",
    summary = "GitHub 連携状態取得",
    params(
        ("tenant_id" = Uuid, Path),
        ("project_id" = Uuid, Path),
    ),
    responses((status = 200, body = GithubIntegrationResponse), CrudErrors)
)]
pub async fn get_github_integration(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<GithubIntegrationResponse>, AppError> {
    state.settings.require_github_app()?;
    auth.require_session()?;
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    require_project_in_tenant(&state, tenant_id, project_id).await?;

    let integration = github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?;

    Ok(Json(match integration {
        Some(row) => GithubIntegrationResponse {
            connected: true,
            repo_owner: Some(row.repo_owner),
            repo_name: Some(row.repo_name),
            connected_at: Some(row.created_at),
        },
        None => GithubIntegrationResponse {
            connected: false,
            repo_owner: None,
            repo_name: None,
            connected_at: None,
        },
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/integration",
    tag = "GitHub",
    summary = "GitHub 連携解除",
    params(
        ("tenant_id" = Uuid, Path),
        ("project_id" = Uuid, Path),
    ),
    responses((status = 204, description = "解除完了"), CrudErrors)
)]
pub async fn delete_github_integration(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let github = state.settings.require_github_app()?;
    auth.require_session()?;
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    require_project_in_tenant(&state, tenant_id, project_id).await?;

    let integration = github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?;

    let Some(row) = integration else {
        return Err(AppError::NotFound);
    };

    // DB を先に削除し、その後 GitHub API で連携解除する。
    // 順序を逆にすると GitHub 側が消えた後に DB 削除が失敗した場合に「connected」が残る。
    let installation_id = row.installation_id;
    let active: github_integrations::ActiveModel = row.into();
    active.delete(&state.db).await?;

    if let Err(e) =
        github_api::delete_app_installation(&state.http_client, github, installation_id).await
    {
        // GitHub 側の削除失敗はログのみ。DB は既に削除済みなので UI には「未連携」と表示される。
        tracing::warn!(
            error = %e,
            installation_id,
            "github delete_app_installation failed; db record already removed"
        );
    }

    Ok(StatusCode::NO_CONTENT)
}

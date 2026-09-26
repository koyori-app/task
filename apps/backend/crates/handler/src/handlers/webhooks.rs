//! 外部向け Webhook の管理 API（docs/features/tasks/10.webhooks.md §6）。
//!
//! 読み取りは `read:project`、変更は `admin:project` + プロジェクト Admin（またはテナントオーナー）。
//! 有効 / 無効の切り替えは専用の口を作らず、PUT の `is_active` で行う。

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, prelude::Uuid,
};

use crate::AppState;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::handlers::project_members::require_project_admin;
use crate::openapi::CrudErrors;
use entity::{scopes::Scope, webhook_deliveries, webhooks};
use payload::webhooks::*;
use service::webhooks::{
    FORMAT_JSON, encrypt_secret, validate_events, validate_format, validate_secret, validate_url,
};

const DEFAULT_DELIVERY_LIMIT: u64 = 50;

async fn ensure_read_access(
    state: &AppState,
    auth: &AuthUser,
    tenant_id: Uuid,
    project_id: Uuid,
) -> Result<(), AppError> {
    auth.require_scope(Scope::ReadProject)?;
    auth.ensure_tenant_access(state, tenant_id, Some(project_id))
        .await
}

async fn ensure_admin_access(
    state: &AppState,
    auth: &AuthUser,
    tenant_id: Uuid,
    project_id: Uuid,
) -> Result<(), AppError> {
    auth.require_scope(Scope::AdminProject)?;
    auth.ensure_tenant_access(state, tenant_id, Some(project_id))
        .await?;
    require_project_admin(state, tenant_id, project_id, auth.user_id).await
}

/// 他プロジェクトの ID を渡されても存在を漏らさない（404）。
async fn find_webhook(
    state: &AppState,
    project_id: Uuid,
    id: Uuid,
) -> Result<webhooks::Model, AppError> {
    webhooks::Entity::find_by_id(id)
        .filter(webhooks::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    operation_id = "list_webhooks",
    tag = "Webhooks",
    summary = "Webhook 一覧（secret は返さない）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "Webhook 一覧", body = [WebhookResponse]),
        CrudErrors,
    )
)]
pub async fn list_webhooks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<WebhookResponse>>, AppError> {
    ensure_read_access(&state, &auth, tenant_id, project_id).await?;
    let rows = webhooks::Entity::find()
        .filter(webhooks::Column::ProjectId.eq(project_id))
        .order_by_asc(webhooks::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    operation_id = "create_webhook",
    tag = "Webhooks",
    summary = "Webhook を作成（secret はこの応答でだけ平文で返す）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = CreateWebhookRequest,
    responses(
        (status = 201, description = "作成した Webhook", body = CreateWebhookResponse),
        CrudErrors,
    )
)]
pub async fn create_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateWebhookRequest>>,
) -> Result<(StatusCode, Json<CreateWebhookResponse>), AppError> {
    ensure_admin_access(&state, &auth, tenant_id, project_id).await?;
    let format = payload.format.unwrap_or_else(|| FORMAT_JSON.to_string());
    validate_url(&state.settings, &payload.url).await?;
    validate_secret(&payload.secret)?;
    validate_events(&payload.events)?;
    validate_format(&format)?;

    let model = webhooks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        url: Set(payload.url),
        secret_enc: Set(encrypt_secret(&state.settings, &payload.secret)?),
        events: Set(payload.events),
        format: Set(format),
        is_active: Set(true),
        failure_streak: Set(0),
        created_by: Set(auth.user_id),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&state.db)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateWebhookResponse {
            webhook: WebhookResponse::for_admin(model),
            secret: payload.secret,
        }),
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    operation_id = "update_webhook",
    tag = "Webhooks",
    summary = "Webhook を更新（有効 / 無効の切り替えも `is_active` で行う）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "Webhook ID"),
    ),
    request_body = UpdateWebhookRequest,
    responses(
        (status = 200, description = "更新後の Webhook", body = WebhookResponse),
        CrudErrors,
    )
)]
pub async fn update_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateWebhookRequest>>,
) -> Result<Json<WebhookResponse>, AppError> {
    ensure_admin_access(&state, &auth, tenant_id, project_id).await?;
    let webhook = find_webhook(&state, project_id, id).await?;

    let mut active: webhooks::ActiveModel = webhook.into();
    if let Some(url) = payload.url {
        validate_url(&state.settings, &url).await?;
        active.url = Set(url);
    }
    if let Some(secret) = payload.secret {
        validate_secret(&secret)?;
        active.secret_enc = Set(encrypt_secret(&state.settings, &secret)?);
    }
    if let Some(events) = payload.events {
        validate_events(&events)?;
        active.events = Set(events);
    }
    if let Some(format) = payload.format {
        validate_format(&format)?;
        active.format = Set(format);
    }
    if let Some(is_active) = payload.is_active {
        active.is_active = Set(is_active);
        if is_active {
            // 止まった Webhook を戻したら数え直す（すぐにまた止まらないように）
            active.failure_streak = Set(0);
        }
    }
    Ok(Json(WebhookResponse::for_admin(
        active.update(&state.db).await?,
    )))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    operation_id = "delete_webhook",
    tag = "Webhooks",
    summary = "Webhook を削除（配信履歴も消える）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "Webhook ID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    ensure_admin_access(&state, &auth, tenant_id, project_id).await?;
    let webhook = find_webhook(&state, project_id, id).await?;
    webhooks::Entity::delete_by_id(webhook.id)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}/deliveries",
    operation_id = "list_webhook_deliveries",
    tag = "Webhooks",
    summary = "配信履歴（新しい順）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "Webhook ID"),
        DeliveryListQuery,
    ),
    responses(
        (status = 200, description = "配信履歴", body = [WebhookDeliveryResponse]),
        CrudErrors,
    )
)]
pub async fn list_webhook_deliveries(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Query(query)): Valid<Query<DeliveryListQuery>>,
) -> Result<Json<Vec<WebhookDeliveryResponse>>, AppError> {
    ensure_read_access(&state, &auth, tenant_id, project_id).await?;
    let webhook = find_webhook(&state, project_id, id).await?;
    let rows = webhook_deliveries::Entity::find()
        .filter(webhook_deliveries::Column::WebhookId.eq(webhook.id))
        .order_by_desc(webhook_deliveries::Column::CreatedAt)
        .order_by_desc(webhook_deliveries::Column::Id)
        .limit(query.limit.unwrap_or(DEFAULT_DELIVERY_LIMIT))
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/deliveries/{delivery_id}/redeliver",
    operation_id = "redeliver_webhook_delivery",
    tag = "Webhooks",
    summary = "配信を手動で再送（同じ payload で新しい配信を作る。元の行は変えない）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = Uuid, Path, description = "Webhook ID"),
        ("delivery_id" = Uuid, Path, description = "配信ID"),
    ),
    responses(
        (status = 201, description = "新しく積んだ配信", body = WebhookDeliveryResponse),
        CrudErrors,
    )
)]
pub async fn redeliver_webhook_delivery(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, delivery_id)): Path<(Uuid, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<WebhookDeliveryResponse>), AppError> {
    ensure_admin_access(&state, &auth, tenant_id, project_id).await?;
    let webhook = find_webhook(&state, project_id, id).await?;
    let original = webhook_deliveries::Entity::find_by_id(delivery_id)
        .filter(webhook_deliveries::Column::WebhookId.eq(webhook.id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let now = chrono::Utc::now();
    let model = webhook_deliveries::ActiveModel {
        id: Set(Uuid::new_v4()),
        webhook_id: Set(webhook.id),
        event: Set(original.event),
        payload: Set(original.payload),
        status_code: Set(None),
        attempt: Set(0),
        next_attempt_at: Set(Some(now.into())),
        last_error: Set(None),
        delivered_at: Set(None),
        created_at: Set(now.into()),
    }
    .insert(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(model.into())))
}

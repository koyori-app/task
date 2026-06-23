use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect, prelude::Uuid,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState, entities::audit_logs, error::AppError, extractors::AdminUser, openapi::CrudErrors,
    payload::admin_audit_logs::*,
};

#[derive(Serialize, Deserialize)]
struct AuditLogCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    let payload = serde_json::to_string(&AuditLogCursor { created_at, id }).expect("cursor json");
    URL_SAFE_NO_PAD.encode(payload.as_bytes())
}

fn decode_cursor(cursor: &str) -> Result<AuditLogCursor, AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.trim())
        .map_err(|_| AppError::BadRequest)?;
    let s = String::from_utf8(bytes).map_err(|_| AppError::BadRequest)?;
    serde_json::from_str(&s).map_err(|_| AppError::BadRequest)
}

fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| AppError::BadRequest)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Admin Audit Logs",
    summary = "監査ログ一覧（管理者）",
    params(AuditLogQuery),
    responses(
        (status = 200, description = "監査ログ一覧", body = AuditLogListResponse),
        CrudErrors,
    )
)]
pub async fn list_audit_logs(
    State(state): State<AppState>,
    _admin: AdminUser,
    _headers: HeaderMap,
    Query(params): Query<AuditLogQuery>,
) -> Result<Json<AuditLogListResponse>, AppError> {
    let limit = params.limit.clamp(1, 200) as u64;
    let fetch = limit + 1;

    let mut query = audit_logs::Entity::find();

    if let Some(action) = &params.action {
        if !action.is_empty() {
            query = query.filter(audit_logs::Column::Action.starts_with(action.as_str()));
        }
    }
    if let Some(rt) = &params.resource_type {
        query = query.filter(audit_logs::Column::ResourceType.eq(rt.as_str()));
    }
    if let Some(rid) = &params.resource_id {
        query = query.filter(audit_logs::Column::ResourceId.eq(rid.as_str()));
    }
    if let Some(actor_id) = params.actor_id {
        query = query.filter(audit_logs::Column::ActorId.eq(actor_id));
    }
    if let Some(tenant_id) = params.tenant_id {
        query = query.filter(audit_logs::Column::TenantId.eq(tenant_id));
    }
    if let Some(from) = &params.from {
        let dt = parse_rfc3339(from)?;
        query = query.filter(audit_logs::Column::CreatedAt.gte(dt));
    }
    if let Some(to) = &params.to {
        let dt = parse_rfc3339(to)?;
        query = query.filter(audit_logs::Column::CreatedAt.lte(dt));
    }

    if let Some(cursor_str) = &params.cursor {
        let c = decode_cursor(cursor_str)?;
        query = query.filter(
            Condition::any()
                .add(audit_logs::Column::CreatedAt.lt(c.created_at))
                .add(
                    Condition::all()
                        .add(audit_logs::Column::CreatedAt.eq(c.created_at))
                        .add(audit_logs::Column::Id.lt(c.id)),
                ),
        );
    }

    let rows = query
        .order_by_desc(audit_logs::Column::CreatedAt)
        .order_by_desc(audit_logs::Column::Id)
        .limit(fetch)
        .all(&state.db)
        .await?;

    let has_more = rows.len() > limit as usize;
    let logs: Vec<_> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        logs.last().map(|log| encode_cursor(log.created_at, log.id))
    } else {
        None
    };

    Ok(Json(AuditLogListResponse { logs, next_cursor }))
}

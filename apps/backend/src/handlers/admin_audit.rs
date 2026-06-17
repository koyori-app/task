//! 管理者操作の監査ログ記録ヘルパー。

use axum::http::HeaderMap;
use sea_orm::prelude::{Json, Uuid};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};

use crate::entities::audit_logs;

pub async fn record_audit(
    db: &DatabaseConnection,
    actor_id: Uuid,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    tenant_id: Option<Uuid>,
    metadata: Option<serde_json::Value>,
    headers: &HeaderMap,
) -> Result<(), sea_orm::DbErr> {
    let ip_address = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    audit_logs::ActiveModel {
        id: Set(Uuid::new_v4()),
        actor_id: Set(Some(actor_id)),
        actor_type: Set("user".to_string()),
        action: Set(action.to_string()),
        resource_type: Set(resource_type.to_string()),
        resource_id: Set(resource_id.to_string()),
        tenant_id: Set(tenant_id),
        metadata: Set(metadata.map(Json::from)),
        ip_address: Set(ip_address),
        user_agent: Set(user_agent),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(db)
    .await?;

    Ok(())
}

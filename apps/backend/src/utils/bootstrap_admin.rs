//! 管理者ブートストラップ（仕様書 admin.md 4 節）

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::json;
use uuid::Uuid;

use crate::entities::{audit_logs, users};
use crate::utils::email::normalize_email;

/// `BOOTSTRAP_ADMIN_EMAIL` 設定時、管理者が 0 人なら対象ユーザーを昇格する。
pub async fn bootstrap_admin_email(
    db: &DatabaseConnection,
    email: &str,
) -> Result<(), sea_orm::DbErr> {
    let admin_count = users::Entity::find()
        .filter(users::Column::IsAdmin.eq(true))
        .count(db)
        .await?;

    if admin_count > 0 {
        tracing::info!("Admin already exists, skipping bootstrap");
        return Ok(());
    }

    let normalized = normalize_email(email);
    let Some(user) = users::Entity::find()
        .filter(users::Column::Email.eq(normalized))
        .one(db)
        .await?
    else {
        tracing::warn!(email = %email, "BOOTSTRAP_ADMIN_EMAIL set but no matching user found");
        return Ok(());
    };

    let mut active: users::ActiveModel = user.clone().into();
    active.is_admin = Set(true);
    active.update(db).await?;

    let log = audit_logs::ActiveModel {
        id: Set(Uuid::new_v4()),
        actor_id: Set(None),
        actor_type: Set("system".to_string()),
        action: Set("user.admin.grant".to_string()),
        resource_type: Set("user".to_string()),
        resource_id: Set(user.id.to_string()),
        tenant_id: Set(None),
        metadata: Set(Some(json!({ "user_id": user.id.to_string() }))),
        ip_address: Set(None),
        user_agent: Set(None),
        created_at: Set(chrono::Utc::now()),
    };
    log.insert(db).await?;

    tracing::info!(user_id = %user.id, "bootstrap: promoted user to admin");
    Ok(())
}

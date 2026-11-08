//! 管理者ブートストラップ（仕様書 admin.md 4 節）

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::json;
use uuid::Uuid;

use crate::email::normalize_email;
use entity::{audit_logs, oauth_connections, users};

/// `BOOTSTRAP_ADMIN_EMAIL` 設定時、管理者が 0 人なら対象ユーザーを昇格する。
///
/// 昇格するのは、email が確認済み（`email_verified`）で、その確認の出所が確かな利用者だけ。
/// `gitlab_selfhosted` の連携を持つ利用者は、確認の印を運用者の外の instance から受け取った
/// ことがありうるゆえ昇格しない（password の登録の確認 mail・GitHub・gitlab.com・Google・
/// 運用者の OIDC で確かめた email はこれに当たらぬ）。条件を満たさねば何もせず、警告を残す。
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

    if let Some(reason) = bootstrap_ineligibility(db, &user).await? {
        tracing::warn!(user_id = %user.id, "bootstrap skipped: {reason}");
        return Ok(());
    }

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
        created_at: Set(chrono::Utc::now().into()),
    };
    log.insert(db).await?;

    tracing::info!(user_id = %user.id, "bootstrap: promoted user to admin");
    Ok(())
}

/// 管理者に昇格させてはならぬ訳。昇格してよければ `None`。
pub async fn bootstrap_ineligibility(
    db: &DatabaseConnection,
    user: &users::Model,
) -> Result<Option<&'static str>, sea_orm::DbErr> {
    if !user.email_verified {
        return Ok(Some("the email of the matching user is not verified"));
    }
    let selfhosted_connections = oauth_connections::Entity::find()
        .filter(oauth_connections::Column::UserId.eq(user.id))
        .filter(oauth_connections::Column::Provider.eq("gitlab_selfhosted"))
        .count(db)
        .await?;
    if selfhosted_connections > 0 {
        return Ok(Some(
            "the matching user has a gitlab_selfhosted connection; verify the email through a password sign-up or another provider",
        ));
    }
    Ok(None)
}

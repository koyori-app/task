//! ログイン成功後のセッション確立（パスワード / OAuth 共通）。
//! 2FA スキーマ未導入環境では runtime 検査で no-op し、導入後は half_authed を設定する。

use axum_session::Session;
use axum_session_redispool::SessionRedisPool;
use chrono::Utc;
use sea_orm::DatabaseConnection;

use common::db::{column_exists, query_one_bool, table_exists};
use entity::users;

#[derive(Debug, Clone, Copy)]
pub struct Login2faFlags {
    pub requires_2fa: bool,
    pub requires_2fa_setup: bool,
}

impl Login2faFlags {
    pub fn needs_second_factor(self) -> bool {
        self.requires_2fa || self.requires_2fa_setup
    }
}

async fn user_has_active_2fa(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
) -> Result<bool, sea_orm::DbErr> {
    if !column_exists(db, "users", "totp_enabled").await? {
        return Ok(false);
    }
    if !table_exists(db, "totp_credentials").await? {
        return Ok(false);
    }

    let totp_enabled = query_one_bool(
        db,
        "SELECT totp_enabled FROM users WHERE id = ?",
        vec![user_id.into()],
    )
    .await?;
    if !totp_enabled {
        return Ok(false);
    }

    query_one_bool(
        db,
        "SELECT is_verified FROM totp_credentials WHERE user_id = ?",
        vec![user_id.into()],
    )
    .await
}

async fn user_in_require_2fa_tenant(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
) -> Result<bool, sea_orm::DbErr> {
    if !column_exists(db, "tenants", "require_2fa").await? {
        return Ok(false);
    }

    if query_one_bool(
        db,
        "SELECT COALESCE(BOOL_OR(require_2fa), false) FROM tenants WHERE owner_id = ?",
        vec![user_id.into()],
    )
    .await?
    {
        return Ok(true);
    }

    if !table_exists(db, "project_members").await? {
        return Ok(false);
    }

    query_one_bool(
        db,
        "SELECT COALESCE(BOOL_OR(t.require_2fa), false)
         FROM project_members pm
         INNER JOIN projects p ON p.id = pm.project_id
         INNER JOIN tenants t ON t.id = p.tenant_id
         WHERE pm.user_id = ?",
        vec![user_id.into()],
    )
    .await
}

pub async fn login_2fa_flags(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
) -> Result<Login2faFlags, sea_orm::DbErr> {
    let requires_2fa = user_has_active_2fa(db, user_id).await?;
    let requires_2fa_setup = if requires_2fa {
        false
    } else {
        user_in_require_2fa_tenant(db, user_id).await?
    };
    Ok(Login2faFlags {
        requires_2fa,
        requires_2fa_setup,
    })
}

/// 第一認証成功後のセッション確立。2FA 必須時は `Some(Login2faFlags)` を返す。
pub async fn establish_login_session(
    session: &Session<SessionRedisPool>,
    db: &DatabaseConnection,
    user: &users::Model,
) -> Result<Option<Login2faFlags>, sea_orm::DbErr> {
    let flags = login_2fa_flags(db, user.id).await?;
    session.renew();
    session.set("issued_at_ms", Utc::now().timestamp_millis());
    session.set("user_id", user.id);
    if flags.needs_second_factor() {
        session.set("half_authed", true);
        return Ok(Some(flags));
    }
    session.set("half_authed", false);
    Ok(None)
}

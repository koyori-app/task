//! テナント BOT の所属と鍵の管理。
use chrono::{DateTime, Duration, Utc};
use common::error::AppError;
use entity::scopes::ScopeList;
use entity::{audit_logs, personal_tokens, projects, tenant_members, tenants, users};
use payload::{
    bots::*,
    personal_tokens::{CreatePersonalTokenResponse, PersonalTokenResponse},
};
use sea_orm::prelude::Uuid;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};

/// BOT の PAT の期限の上限（発行から 365 日）に足す猶予。画面は端末の時計から「1 年」を作るので、
/// 端末がサーバーより進んでいても上限で落とさない。
const BOT_TOKEN_EXPIRY_SKEW: Duration = Duration::minutes(5);

/// BOT の PAT の期限は必須で、今より後、かつ 365 日（と猶予）以内。
fn bot_token_expiry_is_valid(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    expires_at > now && expires_at <= now + Duration::days(365) + BOT_TOKEN_EXPIRY_SKEW
}

// 管理者の降格や停止が判定と書き込みの間へ割り込まないよう、同じ取引で行を押さえる。
async fn require_admin<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    actor_id: Uuid,
) -> Result<(), AppError> {
    let actor = users::Entity::find_by_id(actor_id)
        .lock_shared()
        .one(db)
        .await?
        .ok_or(AppError::Forbidden)?;
    if actor.is_bot() || actor.is_suspended {
        return Err(AppError::Forbidden);
    }
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .lock_shared()
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id == actor_id {
        return Ok(());
    }
    let member = tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .filter(tenant_members::Column::UserId.eq(actor_id))
        .lock_shared()
        .one(db)
        .await?;
    if member.is_some_and(|m| m.role == tenant_members::TenantRole::Admin) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn bot(
    txn: &DatabaseTransaction,
    tenant_id: Uuid,
    bot_id: Uuid,
) -> Result<users::Model, AppError> {
    users::Entity::find_by_id(bot_id)
        .filter(users::Column::AccountKind.eq("bot"))
        .filter(users::Column::BotTenantId.eq(tenant_id))
        .lock_exclusive()
        .one(txn)
        .await?
        .ok_or(AppError::NotFound)
}

async fn audit(
    txn: &DatabaseTransaction,
    actor_id: Uuid,
    tenant_id: Uuid,
    bot_id: Uuid,
    action: &str,
    token_id: Option<Uuid>,
) -> Result<(), AppError> {
    audit_logs::ActiveModel {
        id: Set(Uuid::new_v4()),
        actor_id: Set(Some(actor_id)),
        actor_type: Set("user".into()),
        action: Set(action.into()),
        resource_type: Set("bot".into()),
        resource_id: Set(bot_id.to_string()),
        tenant_id: Set(Some(tenant_id)),
        metadata: Set(Some(serde_json::json!({"token_id": token_id}))),
        ip_address: Set(None),
        user_agent: Set(None),
        created_at: Set(Utc::now().into()),
    }
    .insert(txn)
    .await?;
    Ok(())
}

fn response(user: users::Model, role: tenant_members::TenantRole, tenant_id: Uuid) -> BotResponse {
    BotResponse {
        id: user.id,
        tenant_id,
        username: user.username,
        account_kind: user.account_kind,
        is_suspended: user.is_suspended,
        role,
    }
}

pub async fn list_bots(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    actor_id: Uuid,
) -> Result<Vec<BotResponse>, AppError> {
    require_admin(db, tenant_id, actor_id).await?;
    let bots = users::Entity::find()
        .filter(users::Column::BotTenantId.eq(tenant_id))
        .filter(users::Column::AccountKind.eq("bot"))
        .order_by_asc(users::Column::Username)
        .order_by_asc(users::Column::Id)
        .all(db)
        .await?;
    let members = tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .all(db)
        .await?;
    let mut result = Vec::new();
    for user in bots {
        let role = members
            .iter()
            .find(|m| m.user_id == user.id)
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("BOT membership missing")))?
            .role
            .clone();
        result.push(response(user, role, tenant_id));
    }
    Ok(result)
}

pub async fn create_bot(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    actor_id: Uuid,
    payload: CreateBotRequest,
) -> Result<BotResponse, AppError> {
    if payload.username.trim().is_empty() {
        return Err(AppError::BadRequest);
    }
    let txn = db.begin().await?;
    require_admin(&txn, tenant_id, actor_id).await?;
    let id = Uuid::new_v4();
    let user = users::ActiveModel {
        id: Set(id),
        username: Set(payload.username.trim().to_string()),
        account_kind: Set("bot".into()),
        bot_tenant_id: Set(Some(tenant_id)),
        email: Set(format!("{id}@bots.task.invalid")),
        email_verified: Set(false),
        password_hash: Set(None),
        is_admin: Set(false),
        is_suspended: Set(false),
        sessions_revoked_at: Set(None),
        totp_enabled: Set(false),
        bio: Set(None),
        avatar_url: Set(None),
        avatar_storage_key: Set(None),
    }
    .insert(&txn)
    .await?;
    tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(id),
        role: Set(payload.role.clone()),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(&txn)
    .await?;
    audit(&txn, actor_id, tenant_id, id, "bot.created", None).await?;
    txn.commit().await?;
    Ok(response(user, payload.role, tenant_id))
}

pub async fn update_bot(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    bot_id: Uuid,
    actor_id: Uuid,
    payload: UpdateBotRequest,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    require_admin(&txn, tenant_id, actor_id).await?;
    let user = bot(&txn, tenant_id, bot_id).await?;
    let mut active: users::ActiveModel = user.into();
    active.is_suspended = Set(payload.is_suspended);
    active.update(&txn).await?;
    if payload.is_suspended {
        personal_tokens::Entity::update_many()
            .col_expr(personal_tokens::Column::Revoked, Expr::value(true))
            .filter(personal_tokens::Column::UserId.eq(bot_id))
            .exec(&txn)
            .await?;
    }
    audit(
        &txn,
        actor_id,
        tenant_id,
        bot_id,
        if payload.is_suspended {
            "bot.suspended"
        } else {
            "bot.resumed"
        },
        None,
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

pub async fn list_bot_tokens(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    bot_id: Uuid,
    actor_id: Uuid,
) -> Result<Vec<PersonalTokenResponse>, AppError> {
    let txn = db.begin().await?;
    require_admin(&txn, tenant_id, actor_id).await?;
    bot(&txn, tenant_id, bot_id).await?;
    let tokens = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(bot_id))
        .filter(personal_tokens::Column::TenantId.eq(tenant_id))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .order_by_asc(personal_tokens::Column::Name)
        .order_by_asc(personal_tokens::Column::Id)
        .all(&txn)
        .await?;
    let result = tokens
        .into_iter()
        .map(PersonalTokenResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Internal(e.into()))?;
    txn.commit().await?;
    Ok(result)
}

pub async fn create_bot_token(
    db: &DatabaseConnection,
    token_secret: &str,
    tenant_id: Uuid,
    bot_id: Uuid,
    actor_id: Uuid,
    payload: CreateBotTokenRequest,
) -> Result<CreatePersonalTokenResponse, AppError> {
    if !bot_token_expiry_is_valid(payload.expires_at, Utc::now()) || payload.name.trim().is_empty()
    {
        return Err(AppError::BadRequest);
    }
    let txn = db.begin().await?;
    require_admin(&txn, tenant_id, actor_id).await?;
    if bot(&txn, tenant_id, bot_id).await?.is_suspended {
        return Err(AppError::Forbidden);
    }
    if let Some(ref ids) = payload.project_ids {
        if ids.is_empty() {
            return Err(AppError::BadRequest);
        }
        let unique: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
        let found = projects::Entity::find()
            .filter(projects::Column::TenantId.eq(tenant_id))
            .filter(projects::Column::Id.is_in(unique.iter().copied()))
            .all(&txn)
            .await?;
        if found.len() != unique.len() {
            return Err(AppError::Forbidden);
        }
    }
    let (secret, hash) = crate::auth::generate_personal_token(token_secret)
        .map_err(|e| AppError::Internal(e.into()))?;
    let token_id = Uuid::new_v4();
    let token = personal_tokens::ActiveModel {
        id: Set(token_id),
        user_id: Set(bot_id),
        tenant_id: Set(tenant_id),
        name: Set(payload.name),
        token_last_four: Set(secret[secret.len() - 4..].into()),
        token_hash: Set(hash),
        scopes: Set(ScopeList(payload.scopes)),
        allowed_project_ids: Set(payload.project_ids.map(|v| serde_json::json!(v))),
        expires_at: Set(Some(payload.expires_at.into())),
        last_used_at: Set(None),
        revoked: Set(false),
    }
    .insert(&txn)
    .await?;
    audit(
        &txn,
        actor_id,
        tenant_id,
        bot_id,
        "bot.token_issued",
        Some(token_id),
    )
    .await?;
    txn.commit().await?;
    CreatePersonalTokenResponse::new(secret, token).map_err(|e| AppError::Internal(e.into()))
}

pub async fn revoke_bot_token(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    bot_id: Uuid,
    token_id: Uuid,
    actor_id: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    require_admin(&txn, tenant_id, actor_id).await?;
    bot(&txn, tenant_id, bot_id).await?;
    let token = personal_tokens::Entity::find_by_id(token_id)
        .filter(personal_tokens::Column::UserId.eq(bot_id))
        .filter(personal_tokens::Column::TenantId.eq(tenant_id))
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut active: personal_tokens::ActiveModel = token.into();
    active.revoked = Set(true);
    active.update(&txn).await?;
    audit(
        &txn,
        actor_id,
        tenant_id,
        bot_id,
        "bot.token_revoked",
        Some(token_id),
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

/// 既存のテナント一括失効でも BOT の鍵と操作者を監査に残す。
pub async fn revoke_all_tenant_tokens(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    actor_id: Uuid,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    require_admin(&txn, tenant_id, actor_id).await?;
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .lock_shared()
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id != actor_id {
        return Err(AppError::Forbidden);
    }
    let bots = users::Entity::find()
        .filter(users::Column::BotTenantId.eq(tenant_id))
        .filter(users::Column::AccountKind.eq("bot"))
        .lock_exclusive()
        .all(&txn)
        .await?;
    let bot_ids: std::collections::HashSet<Uuid> = bots.into_iter().map(|user| user.id).collect();
    let tokens = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::TenantId.eq(tenant_id))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .all(&txn)
        .await?;
    personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::Revoked, Expr::value(true))
        .filter(personal_tokens::Column::TenantId.eq(tenant_id))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .exec(&txn)
        .await?;
    for token in tokens {
        if bot_ids.contains(&token.user_id) {
            audit(
                &txn,
                actor_id,
                tenant_id,
                token.user_id,
                "bot.token_revoked",
                Some(token.id),
            )
            .await?;
        }
    }
    txn.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn bot_token_expiry_allows_five_minutes_of_clock_skew_past_a_year() {
        let now = Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap();
        let limit = now + Duration::days(365) + Duration::minutes(5);
        assert!(bot_token_expiry_is_valid(limit, now));
        assert!(!bot_token_expiry_is_valid(
            limit + Duration::seconds(1),
            now
        ));
        assert!(bot_token_expiry_is_valid(now + Duration::seconds(1), now));
        assert!(!bot_token_expiry_is_valid(now, now));
    }
}

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, prelude::Uuid};

use crate::error::AppError;
use entity::tenant_members::{self, TenantRole};

// 所属・公開範囲の問い合わせは service::access の実装を再公開する（同一実装の重複を作らない）。
// 操作認可の `require_*` はこのモジュールに置く。
pub use service::access::{
    explicit_member_project_ids, guest_tenant_ids, is_project_member,
    is_shared_project_explicit_member, is_tenant_member, project_is_open_or_member,
    visible_project_ids,
};
pub use service::drive::is_tenant_owner;

/// 主（owner）だけに許す口の共通判定。テナントが無ければ NotFound、主でなければ Forbidden
/// （`is_tenant_owner` が存在確認を兼ねる）。主の定義が変わる時はここ一箇所を直す——
/// handler ごとに owner_id を直に比べる手書きを作らないこと。
pub async fn require_tenant_owner<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_owner(db, tenant_id, user_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// `tenant_members` の行がテナント `Admin` か（オーナーは見ない。オーナー込みは [`is_tenant_admin`]）。
pub async fn has_tenant_admin_role<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let member = tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .filter(tenant_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;
    Ok(member.is_some_and(|m| m.role == TenantRole::Admin))
}

/// オーナーまたはテナント `Admin` か。テナントが無ければ NotFound（`is_tenant_owner` が存在確認を兼ねる）。
pub async fn is_tenant_admin<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    if is_tenant_owner(db, tenant_id, user_id).await? {
        return Ok(true);
    }
    has_tenant_admin_role(db, tenant_id, user_id).await
}

/// オーナーとテナント `Admin` に許す口の共通判定（メンバー管理・招待・共有プロジェクトの作成・更新・削除）。
pub async fn require_tenant_admin<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_admin(db, tenant_id, user_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// **指定した利用者**がそのプロジェクトに入れるかを確認する。
///
/// リクエスト元自身の認可は `AuthUser::ensure_tenant_access` が同じ判定を含んでいるので、
/// ここを重ねて呼ぶ必要はない。担当者の追加など、自分以外を検証するときだけ使う。
pub async fn require_project_access<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_owner(db, tenant_id, user_id).await? {
        return Ok(());
    }
    if !is_tenant_member(db, tenant_id, user_id).await? {
        return Err(AppError::Forbidden);
    }
    if project_is_open_or_member(db, project_id, user_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

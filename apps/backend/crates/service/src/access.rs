//! テナント / プロジェクトのアクセス判定（#568）。
//!
//! 認可（handler）と通知の宛先抽出（service）が同じルールを見る必要があるため、
//! ここに一本化する。handler 側は `auth_helpers` が再公開している。

use std::collections::HashSet;

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, prelude::Uuid};

use crate::drive::is_tenant_owner;
use common::error::AppError;
use entity::{project_members, projects, tenant_members, tenants};

/// テナントメンバーかどうか（オーナーは含まない）。
pub async fn is_tenant_member<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    Ok(tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .filter(tenant_members::Column::UserId.eq(user_id))
        .one(db)
        .await?
        .is_some())
}

/// プロジェクトメンバーとして明示指定されているか（テナント所属も公開規則も見ない）。
///
/// テナントに行が無い利用者（project-only の客分）を名指しのプロジェクトへ通す判定にも
/// 使うため、「メンバー未指定＝テナント全体に開放」の規則はここでは扱わない。
pub async fn is_project_member<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    Ok(project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::UserId.eq(user_id))
        .one(db)
        .await?
        .is_some())
}

/// 客分の口になる明示 ACE は、共有プロジェクトの行だけ。
///
/// 個人プロジェクト（Inbox）の本人の行は `my_tasks::seed_personal_project_defaults` が
/// 自動で作るもので、管理者が手で置いた ACE ではない。テナントに居る間の入口は
/// `project_is_open_or_member` の `is_personal` 分岐（本人なら通す）が担うので、
/// この行は継承（在籍）が前提の付随物にすぎない。除名の後まで効かせると、
/// テナントを去った人がそのテナントの Inbox に客分として入り続け、テナント一覧に
/// Guest として出て、2FA 強制の対象にもなる。客分の判定はすべてここを通し、
/// 個人プロジェクトを外す（docs/dev/design/tenant-project-authz.md の「除名が消すもの」）
pub async fn is_shared_project_explicit_member<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    if !is_project_member(db, project_id, user_id).await? {
        return Ok(false);
    }
    Ok(projects::Entity::find_by_id(project_id)
        .one(db)
        .await?
        .is_some_and(|p| !p.is_personal))
}

/// project-only の客分として関わるテナント（自分が `project_members` に明示指定されている
/// 共有プロジェクトを持つテナント）の id 集合。テナント一覧の印付けに使う。
///
/// オーナー・テナントメンバーであるテナントもここに含まれうる（明示指定は絞り込みとしても
/// 使われるため）。除く判定は呼び出し側で行う。
/// 個人プロジェクトの行は数えない（`is_shared_project_explicit_member` の doc）。
pub async fn guest_tenant_ids<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let project_ids: Vec<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::UserId.eq(user_id))
        .select_only()
        .column(project_members::Column::ProjectId)
        .into_tuple()
        .all(db)
        .await?;
    if project_ids.is_empty() {
        return Ok(HashSet::new());
    }
    Ok(projects::Entity::find()
        .filter(projects::Column::Id.is_in(project_ids))
        .filter(projects::Column::IsPersonal.eq(false))
        .select_only()
        .column(projects::Column::TenantId)
        .distinct()
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect())
}

/// そのテナント配下で自分が `project_members` に明示指定されている共有 project の id 集合。
///
/// project-only の客分に開く一覧系 2 口（プロジェクト一覧・My Tasks）の絞り込みに使う。
/// 公開規則（メンバー未指定＝テナント全体に開放）はここでは見ない —
/// 公開 project は客分に開かないため、明示指定の行だけを数える。
/// 個人プロジェクトの行も数えない（`is_shared_project_explicit_member` の doc）。
pub async fn explicit_member_project_ids<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let project_ids: Vec<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::UserId.eq(user_id))
        .select_only()
        .column(project_members::Column::ProjectId)
        .into_tuple()
        .all(db)
        .await?;
    if project_ids.is_empty() {
        return Ok(HashSet::new());
    }
    Ok(projects::Entity::find()
        .filter(projects::Column::Id.is_in(project_ids))
        .filter(projects::Column::TenantId.eq(tenant_id))
        .filter(projects::Column::IsPersonal.eq(false))
        .select_only()
        .column(projects::Column::Id)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect())
}

/// テナントのメンバーがそのプロジェクトに入れるか。プロジェクトに入れる人の決まりは、
/// すべてこの関数を通す（TASK-501）。**テナントに入れることは呼び出し側で確認済みの前提。**
///
/// - `project_members` に載っている人は、載った役で入れる
/// - 共有プロジェクトがテナントのメンバーを継承する（`inherit_tenant_members` が真）なら、
///   載っていないテナントのメンバーも入れる。載せても他の人は外れない
/// - 継承しないなら、載っている人だけが入れる
/// - 個人プロジェクト（Inbox）は継承の対象外で、本人だけが入れる。作成時には本人が
///   `project_members` に入る（`my_tasks::seed_personal_project_defaults`）が、テナントメンバーの
///   削除や利用者の削除でその行は消えうるため、行の有無に頼らず `is_personal` で閉じる
pub fn tenant_member_can_enter(
    project: &projects::Model,
    user_id: Uuid,
    listed_in_project: bool,
) -> bool {
    if listed_in_project {
        return true;
    }
    if project.is_personal {
        return project.personal_owner_id == Some(user_id);
    }
    project.inherit_tenant_members
}

/// プロジェクト単位のアクセス可否。**テナントに入れることは呼び出し側で確認済みの前提。**
///
/// 決まりは [`tenant_member_can_enter`] にある。
pub async fn project_is_open_or_member<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        return Ok(false);
    };
    let listed = is_project_member(db, project_id, user_id).await?;
    Ok(tenant_member_can_enter(&project, user_id, listed))
}

/// そのプロジェクトのタスクに担当者として指定できる利用者。
///
/// 判定規則は handler の `require_project_access` と同じ:
/// テナントオーナーは常に可、テナントメンバーは `project_is_open_or_member` を満たせば可。
/// 1 人ずつ問い合わせると人数分のクエリになるため、候補一覧はここで集合として返す。
///
/// **テナントに入れることは呼び出し側で確認済みの前提。**
pub async fn assignable_user_ids<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    project_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    let Some(tenant) = tenants::Entity::find_by_id(tenant_id).one(db).await? else {
        return Ok(Vec::new());
    };
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        return Ok(Vec::new());
    };

    // このプロジェクトに明示指定されている利用者。入れるかは tenant_member_can_enter が決める
    let named: HashSet<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .select_only()
        .column(project_members::Column::UserId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect();

    let members: Vec<Uuid> = tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .select_only()
        .column(tenant_members::Column::UserId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?;

    let mut ids = Vec::with_capacity(members.len() + 1);
    let mut seen = HashSet::new();
    // オーナーはプロジェクト指定に関係なく可（require_project_access と同じ）
    if seen.insert(tenant.owner_id) {
        ids.push(tenant.owner_id);
    }
    for user_id in members {
        let allowed = tenant_member_can_enter(&project, user_id, named.contains(&user_id));
        if allowed && seen.insert(user_id) {
            ids.push(user_id);
        }
    }
    Ok(ids)
}

/// 候補プロジェクトのうち、そのユーザーに見えるものを返す。
///
/// `project_is_open_or_member` をプロジェクトごとに呼ぶと件数分のクエリが出るため、
/// 一覧系ではこちらを使う。**テナントに入れることは呼び出し側で確認済みの前提。**
pub async fn visible_project_ids<C: ConnectionTrait>(
    db: &C,
    candidate_ids: Vec<Uuid>,
    user_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    if candidate_ids.is_empty() {
        return Ok(HashSet::new());
    }

    // 自分が指定されているもの
    let mine: HashSet<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.is_in(candidate_ids.clone()))
        .filter(project_members::Column::UserId.eq(user_id))
        .select_only()
        .column(project_members::Column::ProjectId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect();

    // 決まりは 1 件ずつの判定と同じ tenant_member_can_enter で見る
    Ok(projects::Entity::find()
        .filter(projects::Column::Id.is_in(candidate_ids))
        .all(db)
        .await?
        .into_iter()
        .filter(|project| tenant_member_can_enter(project, user_id, mine.contains(&project.id)))
        .map(|project| project.id)
        .collect())
}

/// そのプロジェクトに入れる利用者（テナントオーナーは含まない）。
///
/// 通知やメンションの宛先を絞るために使う。`project_is_open_or_member` と同じルールで、
/// メンバー未指定のプロジェクトはテナントメンバー全員が宛先になる。
pub async fn project_accessible_user_ids<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        return Ok(HashSet::new());
    };

    let tenant_member_ids: HashSet<Uuid> = tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(project.tenant_id))
        .select_only()
        .column(tenant_members::Column::UserId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect();

    let members: HashSet<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .select_only()
        .column(project_members::Column::UserId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect();
    // 個人プロジェクトに本人の行が無い時は、テナントの在籍を問わず本人だけを宛先にする（従来どおり）
    if project.is_personal && members.is_empty() {
        return Ok(project.personal_owner_id.into_iter().collect());
    }

    // テナントから外れた人の行は残る（`tenant_members::remove_member`）ので、宛先はテナントの
    // メンバーから選ぶ。入れるかは tenant_member_can_enter が決める
    Ok(tenant_member_ids
        .into_iter()
        .filter(|user_id| tenant_member_can_enter(&project, *user_id, members.contains(user_id)))
        .collect())
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

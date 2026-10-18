//! コメント本文のメンション（`<@利用者ID>` の印）を、いまの利用者の状態へ引く。
//!
//! 印の形と置き換えは `payload::mentions`（純関数）が持つ。ここは DB を引いて、
//! 「その id がいま、そのプロジェクトで名前を出してよい者か」を判じる。通知の宛先の境
//! （`task_activities::extract_mentions`）と、描画・webhook・CLI の名前の解決が
//! **同じ判定を通る**ように、境の集合はここに一つだけ置く。

use std::collections::{HashMap, HashSet};

use payload::mentions::{MentionMap, MentionStatus, MentionedUser, to_plain};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, QueryTrait,
    prelude::Uuid,
    sea_query::{Expr, ExprTrait, Func},
};

use crate::error::AppError;
use entity::{
    project_members, projects, task_activities, task_assignees, task_comments, tasks,
    tenant_members, tenants, users,
};

/// そのプロジェクトで、メンションの宛先にしてよい（名前を出してよい）者。
/// 通知の境と同じ: プロジェクトに入れる者（`project_accessible_user_ids`）とテナント owner。
pub async fn mention_audience<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    // メンバー未指定のプロジェクトはテナントメンバー全員が対象になる（#568）
    let mut audience = crate::access::project_accessible_user_ids(db, project_id).await?;
    if let Some(project) = projects::Entity::find_by_id(project_id).one(db).await?
        && let Some(tenant) = tenants::Entity::find_by_id(project.tenant_id)
            .one(db)
            .await?
    {
        audience.insert(tenant.owner_id);
    }
    Ok(audience)
}

/// そのテナントに属している・属しておった者（`candidates` のうち）。
///
/// **名前を返してよい範囲の境**はここ一つに置く。外れた者・テナントを抜けた者の行は
/// `users` に残るが、テナントの所属の履歴は残らない（`tenant_members` は除名で行ごと消える）ので、
/// 属しておった証拠を次の足跡で引く（いずれも、そのテナントのプロジェクトの中に在る行）:
/// 現在の所属（`tenant_members`）・オーナー（`tenants.owner_id`）・プロジェクトの明示のメンバー
/// （`project_members`。除名後も残る）・タスクの作成者・コメントの書き手・担当者・アクティビティの主。
/// どれも持たぬ者（別のテナントの利用者など）は含まれず、名前は返らぬ。
pub async fn tenant_footprint_ids<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    candidates: &[Uuid],
) -> Result<HashSet<Uuid>, AppError> {
    let mut found = HashSet::new();
    if candidates.is_empty() {
        return Ok(found);
    }
    let candidates: Vec<Uuid> = candidates.to_vec();

    found.extend(
        tenant_members::Entity::find()
            .filter(tenant_members::Column::TenantId.eq(tenant_id))
            .filter(tenant_members::Column::UserId.is_in(candidates.clone()))
            .select_only()
            .column(tenant_members::Column::UserId)
            .into_tuple::<Uuid>()
            .all(db)
            .await?,
    );
    if let Some(tenant) = tenants::Entity::find_by_id(tenant_id).one(db).await?
        && candidates.contains(&tenant.owner_id)
    {
        found.insert(tenant.owner_id);
    }

    let project_ids = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .select_only()
        .column(projects::Column::Id)
        .into_tuple::<Uuid>()
        .all(db)
        .await?;
    if project_ids.is_empty() {
        return Ok(found);
    }
    let tasks_of_tenant = || {
        tasks::Entity::find()
            .select_only()
            .column(tasks::Column::Id)
            .filter(tasks::Column::ProjectId.is_in(project_ids.clone()))
            .into_query()
    };

    found.extend(
        project_members::Entity::find()
            .filter(project_members::Column::ProjectId.is_in(project_ids.clone()))
            .filter(project_members::Column::UserId.is_in(candidates.clone()))
            .select_only()
            .column(project_members::Column::UserId)
            .into_tuple::<Uuid>()
            .all(db)
            .await?,
    );
    found.extend(
        tasks::Entity::find()
            .filter(tasks::Column::ProjectId.is_in(project_ids.clone()))
            .filter(tasks::Column::CreatedBy.is_in(candidates.clone()))
            .select_only()
            .column(tasks::Column::CreatedBy)
            .into_tuple::<Uuid>()
            .all(db)
            .await?,
    );
    found.extend(
        task_comments::Entity::find()
            .filter(task_comments::Column::TaskId.in_subquery(tasks_of_tenant()))
            .filter(task_comments::Column::UserId.is_in(candidates.clone()))
            .select_only()
            .column(task_comments::Column::UserId)
            .into_tuple::<Uuid>()
            .all(db)
            .await?,
    );
    found.extend(
        task_assignees::Entity::find()
            .filter(task_assignees::Column::TaskId.in_subquery(tasks_of_tenant()))
            .filter(task_assignees::Column::UserId.is_in(candidates.clone()))
            .select_only()
            .column(task_assignees::Column::UserId)
            .into_tuple::<Uuid>()
            .all(db)
            .await?,
    );
    found.extend(
        task_activities::Entity::find()
            .filter(task_activities::Column::TaskId.in_subquery(tasks_of_tenant()))
            .filter(task_activities::Column::UserId.is_in(candidates))
            .select_only()
            .column(task_activities::Column::UserId)
            .into_tuple::<Option<Uuid>>()
            .all(db)
            .await?
            .into_iter()
            .flatten(),
    );
    Ok(found)
}

/// 印の id を、いまの状態に引く。返す表は `ids` の全てを含む（引けぬ id は `gone`）。
///
/// 名前を返すのは、いまそのプロジェクトに入れる者（`member`。アイコンも）と、外れた者・テナントを
/// 抜けた者のうちそのテナントに属しておった者（`outside`。名前だけ）。属した形跡の無い利用者
/// （別のテナントの利用者）は `outside` で名前を返さぬ。印の id を知っただけでは、
/// 別のテナントの利用者の名前は引けぬ。通知の宛先（`member` だけ）はこの拡張で変わらぬ。
pub async fn resolve<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    ids: &[Uuid],
) -> Result<MentionMap, AppError> {
    if ids.is_empty() {
        return Ok(MentionMap::new());
    }
    let audience = mention_audience(db, project_id).await?;
    let found = users::Entity::find()
        .filter(users::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    let outsiders: Vec<Uuid> = found
        .iter()
        .map(|user| user.id)
        .filter(|id| !audience.contains(id))
        .collect();
    let belonged = match projects::Entity::find_by_id(project_id).one(db).await? {
        Some(project) if !outsiders.is_empty() => {
            tenant_footprint_ids(db, project.tenant_id, &outsiders).await?
        }
        _ => HashSet::new(),
    };
    let mut map = MentionMap::new();
    for user in found {
        let entry = if audience.contains(&user.id) {
            MentionedUser {
                status: MentionStatus::Member,
                name: Some(user.username),
                avatar_url: user.avatar_url,
            }
        } else {
            MentionedUser {
                status: MentionStatus::Outside,
                name: belonged.contains(&user.id).then_some(user.username),
                avatar_url: None,
            }
        };
        map.insert(user.id, entry);
    }
    for id in ids {
        map.entry(*id).or_insert(MentionedUser {
            status: MentionStatus::Gone,
            name: None,
            avatar_url: None,
        });
    }
    Ok(map)
}

/// 名（`@名` の `名`）を、その名で印を描く者の id へ引く。鍵は小文字にした名。
///
/// 返すのは、`resolve` が名前を返す者と同じ境の者だけ: いまそのプロジェクトに入れる者と、
/// そのテナントに属している・属しておった者（`tenant_footprint_ids`）。別のテナントの利用者は、
/// 名が同じでも返さぬ（名から別のテナントの者を探る道を作らぬ）。名は大小を区別せずに比べ、
/// 同じ名の者が境の中に複数いれば全員の id を返す（描く札の字が同じゆえ、検索でも見分けぬ）。
pub async fn resolve_names<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    names: &[String],
) -> Result<HashMap<String, Vec<Uuid>>, AppError> {
    let mut wanted: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    wanted.sort();
    wanted.dedup();
    if wanted.is_empty() {
        return Ok(HashMap::new());
    }
    let found = users::Entity::find()
        .filter(Expr::expr(Func::lower(Expr::col(users::Column::Username))).is_in(wanted))
        .all(db)
        .await?;
    if found.is_empty() {
        return Ok(HashMap::new());
    }
    let audience = mention_audience(db, project_id).await?;
    let outsiders: Vec<Uuid> = found
        .iter()
        .map(|user| user.id)
        .filter(|id| !audience.contains(id))
        .collect();
    let belonged = match projects::Entity::find_by_id(project_id).one(db).await? {
        Some(project) if !outsiders.is_empty() => {
            tenant_footprint_ids(db, project.tenant_id, &outsiders).await?
        }
        _ => HashSet::new(),
    };
    let mut map: HashMap<String, Vec<Uuid>> = HashMap::new();
    for user in found {
        if audience.contains(&user.id) || belonged.contains(&user.id) {
            map.entry(user.username.to_lowercase())
                .or_default()
                .push(user.id);
        }
    }
    for ids in map.values_mut() {
        ids.sort();
    }
    Ok(map)
}

/// 複数の本文に出る印を、まとめて一度の問い合わせで引く。
pub async fn resolve_in_bodies<'a, C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    bodies: impl IntoIterator<Item = &'a str>,
) -> Result<MentionMap, AppError> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for body in bodies {
        for id in payload::mentions::extract_ids(body) {
            if seen.insert(id) {
                ids.push(id);
            }
        }
    }
    resolve(db, project_id, &ids).await
}

/// 本文を利用者の画面の外（webhook・CLI）へ出せる形（印を `@今の名` に戻した文）にする。
pub async fn plain_body<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    body: &str,
) -> Result<String, AppError> {
    let map = resolve_in_bodies(db, project_id, [body]).await?;
    Ok(to_plain(body, &map))
}

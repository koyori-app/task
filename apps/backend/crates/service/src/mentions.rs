//! コメント本文のメンション（`<@利用者ID>` の印）を、いまの利用者の状態へ引く。
//!
//! 印の形と置き換えは `payload::mentions`（純関数）が持つ。ここは DB を引いて、
//! 「その id がいま、そのプロジェクトで名前を出してよい者か」を判じる。通知の宛先の境
//! （`task_activities::extract_mentions`）と、描画・webhook・CLI の名前の解決が
//! **同じ判定を通る**ように、境の集合はここに一つだけ置く。

use std::collections::HashSet;

use payload::mentions::{MentionMap, MentionStatus, MentionedUser, to_plain};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, prelude::Uuid};

use crate::error::AppError;
use entity::{projects, tenants, users};

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

/// 印の id を、いまの状態に引く。返す表は `ids` の全てを含む（引けぬ id は `gone`）。
///
/// 名前とアイコンを返すのは `member`（いまそのプロジェクトに入れる者）だけ。
/// 外れた者・テナントを抜けた者（`outside`）の名前は、印の id を知っていても引けぬ。
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
                name: None,
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

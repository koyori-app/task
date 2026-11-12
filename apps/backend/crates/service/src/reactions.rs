//! コメントとタスク本体の絵文字リアクション。
//!
//! 通知も `task_activities` の履歴も出さない（docs/features/tasks/4.collaboration.md §6）。

use std::collections::HashMap;

use payload::reactions::{ReactionSummary, ReactionUsersResponse};
use payload::task_comments::CommentUser;
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
    prelude::Uuid,
    sea_query::{LockType, OnConflict},
};

use crate::error::AppError;
use entity::{reactions, task_comments, tasks, users};

/// 1 対象あたりの絵文字の種類の上限。
pub const MAX_EMOJI_KINDS_PER_TARGET: usize = 20;
/// 集計に載せる人数の上限。全員は `users_of` で引く。
pub const SUMMARY_USERS_LIMIT: usize = 5;

const TARGET_TASK: &str = "task";
const TARGET_COMMENT: &str = "comment";

/// リアクションを付ける先。
#[derive(Clone, Copy, Debug)]
pub enum ReactionTarget {
    Task(Uuid),
    Comment(Uuid),
}

impl ReactionTarget {
    fn type_str(self) -> &'static str {
        match self {
            Self::Task(_) => TARGET_TASK,
            Self::Comment(_) => TARGET_COMMENT,
        }
    }

    fn id(self) -> Uuid {
        match self {
            Self::Task(id) | Self::Comment(id) => id,
        }
    }

    fn condition(self) -> Condition {
        Condition::all()
            .add(reactions::Column::TargetType.eq(self.type_str()))
            .add(reactions::Column::TargetId.eq(self.id()))
    }
}

/// そのタスクに属するコメントを引く。無ければ 404。
pub async fn find_comment<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
    comment_id: Uuid,
) -> Result<task_comments::Model, AppError> {
    task_comments::Entity::find_by_id(comment_id)
        .filter(task_comments::Column::TaskId.eq(task_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

/// 対象の行をロックし、まだ付けられる状態かを検める。
///
/// 種類数の上限は「数えてから足す」ので、同じ対象への同時の PUT を直列にしないと超える。
/// コメントの削除とも同じ行で順序が付くので、削除の直後に付いてしまうこともない。
/// FK の `KEY SHARE`（コメント投稿など）とは競合させたくないので `NO KEY UPDATE` で取る。
async fn lock_target<C: ConnectionTrait>(
    txn: &C,
    task_id: Uuid,
    target: ReactionTarget,
) -> Result<(), AppError> {
    match target {
        ReactionTarget::Task(id) => {
            tasks::Entity::find_by_id(id)
                .filter(tasks::Column::DeletedAt.is_null())
                .lock(LockType::NoKeyUpdate)
                .one(txn)
                .await?
                .ok_or(AppError::NotFound)?;
        }
        ReactionTarget::Comment(id) => {
            let comment = task_comments::Entity::find_by_id(id)
                .filter(task_comments::Column::TaskId.eq(task_id))
                .lock(LockType::NoKeyUpdate)
                .one(txn)
                .await?
                .ok_or(AppError::NotFound)?;
            // 返信と同じく、削除済みには付けられない
            if comment.deleted_at.is_some() {
                return Err(AppError::BadRequest);
            }
        }
    }
    Ok(())
}

/// リアクションを付ける。同じ人が同じ絵文字を付け直しても 1 件のまま（冪等）。
pub async fn add(
    db: &DatabaseConnection,
    task_id: Uuid,
    target: ReactionTarget,
    user_id: Uuid,
    emoji: &str,
) -> Result<(), AppError> {
    let txn = db.begin().await?;
    lock_target(&txn, task_id, target).await?;

    let kinds: Vec<String> = reactions::Entity::find()
        .select_only()
        .column(reactions::Column::Emoji)
        .distinct()
        .filter(target.condition())
        .into_tuple()
        .all(&txn)
        .await?;
    // 既にある絵文字は上限に関係なく押せる
    if kinds.len() >= MAX_EMOJI_KINDS_PER_TARGET && !kinds.iter().any(|kind| kind == emoji) {
        return Err(AppError::BadRequestDetail("reaction-limit-exceeded".into()));
    }

    reactions::Entity::insert(reactions::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(task_id),
        target_type: Set(target.type_str().to_owned()),
        target_id: Set(target.id()),
        user_id: Set(user_id),
        emoji: Set(emoji.to_owned()),
        created_at: Set(chrono::Utc::now().into()),
    })
    .on_conflict(
        OnConflict::columns([
            reactions::Column::TargetType,
            reactions::Column::TargetId,
            reactions::Column::UserId,
            reactions::Column::Emoji,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec_without_returning(&txn)
    .await?;
    txn.commit().await?;
    Ok(())
}

/// 自分の分を外す。無ければ何もしない。
pub async fn remove<C: ConnectionTrait>(
    db: &C,
    target: ReactionTarget,
    user_id: Uuid,
    emoji: &str,
) -> Result<(), AppError> {
    reactions::Entity::delete_many()
        .filter(target.condition())
        .filter(reactions::Column::UserId.eq(user_id))
        .filter(reactions::Column::Emoji.eq(emoji))
        .exec(db)
        .await?;
    Ok(())
}

/// 1 つの絵文字の集計。
pub async fn summary<C: ConnectionTrait>(
    db: &C,
    target: ReactionTarget,
    emoji: &str,
    viewer_id: Uuid,
) -> Result<ReactionSummary, AppError> {
    let rows = load_rows(
        db,
        target.condition().add(reactions::Column::Emoji.eq(emoji)),
    )
    .await?;
    let drafts = aggregate(rows.iter(), viewer_id);
    let names = load_user_names(db, drafts.iter().flat_map(|d| d.shown.iter().copied())).await?;
    Ok(drafts
        .into_iter()
        .next()
        .map(|draft| draft.into_summary(&names))
        .unwrap_or_else(|| ReactionSummary::empty(emoji.to_owned())))
}

/// 1 つの絵文字を押した全員（押した順）。
pub async fn users_of<C: ConnectionTrait>(
    db: &C,
    target: ReactionTarget,
    emoji: &str,
) -> Result<ReactionUsersResponse, AppError> {
    let rows = load_rows(
        db,
        target.condition().add(reactions::Column::Emoji.eq(emoji)),
    )
    .await?;
    let names = load_user_names(db, rows.iter().map(|row| row.user_id)).await?;
    Ok(ReactionUsersResponse {
        emoji: emoji.to_owned(),
        count: rows.len() as u64,
        users: rows
            .iter()
            .map(|row| CommentUser::from_lookup(&names, row.user_id))
            .collect(),
    })
}

/// タスク本体のリアクション一覧（最初に付いた順）。
pub async fn summaries_for_task<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
    viewer_id: Uuid,
) -> Result<Vec<ReactionSummary>, AppError> {
    let rows = load_rows(db, ReactionTarget::Task(task_id).condition()).await?;
    let drafts = aggregate(rows.iter(), viewer_id);
    let names = load_user_names(db, drafts.iter().flat_map(|d| d.shown.iter().copied())).await?;
    Ok(drafts
        .into_iter()
        .map(|draft| draft.into_summary(&names))
        .collect())
}

/// タスクの全コメントのリアクションを、コメント id ごとにまとめて引く。
///
/// 行とユーザーを 1 回ずつ引くだけで、コメント数に比例して問い合わせが増えない。
/// 削除済みコメントかは呼び出し側で見る（コメントの行を持っているのは呼び出し側なので）。
pub async fn summaries_for_comments<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
    viewer_id: Uuid,
) -> Result<HashMap<Uuid, Vec<ReactionSummary>>, AppError> {
    let rows = load_rows(
        db,
        Condition::all()
            .add(reactions::Column::TaskId.eq(task_id))
            .add(reactions::Column::TargetType.eq(TARGET_COMMENT)),
    )
    .await?;

    let mut rows_by_comment: HashMap<Uuid, Vec<&reactions::Model>> = HashMap::new();
    for row in &rows {
        rows_by_comment.entry(row.target_id).or_default().push(row);
    }
    let drafts_by_comment: HashMap<Uuid, Vec<Draft>> = rows_by_comment
        .into_iter()
        .map(|(comment_id, rows)| (comment_id, aggregate(rows, viewer_id)))
        .collect();

    let names = load_user_names(
        db,
        drafts_by_comment
            .values()
            .flatten()
            .flat_map(|d| d.shown.iter().copied()),
    )
    .await?;
    Ok(drafts_by_comment
        .into_iter()
        .map(|(comment_id, drafts)| {
            let summaries = drafts
                .into_iter()
                .map(|draft| draft.into_summary(&names))
                .collect();
            (comment_id, summaries)
        })
        .collect())
}

/// 押した順に引く。時刻が並んでも応答の順が揺れないよう id で順序を決める。
async fn load_rows<C: ConnectionTrait>(
    db: &C,
    condition: Condition,
) -> Result<Vec<reactions::Model>, AppError> {
    Ok(reactions::Entity::find()
        .filter(condition)
        .order_by_asc(reactions::Column::CreatedAt)
        .order_by_asc(reactions::Column::Id)
        .all(db)
        .await?)
}

async fn load_user_names<C: ConnectionTrait>(
    db: &C,
    user_ids: impl Iterator<Item = Uuid>,
) -> Result<HashMap<Uuid, (String, Option<String>, String)>, AppError> {
    let mut user_ids: Vec<Uuid> = user_ids.collect();
    user_ids.sort_unstable();
    user_ids.dedup();
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(users::Entity::find()
        .filter(users::Column::Id.is_in(user_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|user| (user.id, (user.username, user.avatar_url, user.account_kind)))
        .collect())
}

/// ユーザー情報を引く前の集計。表に出す人だけを引くために分けている。
#[derive(Debug, PartialEq, Eq)]
struct Draft {
    emoji: String,
    count: u64,
    reacted: bool,
    shown: Vec<Uuid>,
}

impl Draft {
    fn into_summary(
        self,
        names: &HashMap<Uuid, (String, Option<String>, String)>,
    ) -> ReactionSummary {
        ReactionSummary {
            emoji: self.emoji,
            count: self.count,
            reacted: self.reacted,
            users: self
                .shown
                .into_iter()
                .map(|id| CommentUser::from_lookup(names, id))
                .collect(),
        }
    }
}

/// 1 対象の行（押した順）を絵文字ごとに畳む。絵文字の並びは最初に付いた順になる。
fn aggregate<'a>(
    rows: impl IntoIterator<Item = &'a reactions::Model>,
    viewer_id: Uuid,
) -> Vec<Draft> {
    let mut groups: Vec<(&str, Vec<Uuid>)> = Vec::new();
    for row in rows {
        // 種類数には上限があるので線形探索で足りる
        match groups.iter_mut().find(|(emoji, _)| *emoji == row.emoji) {
            Some((_, user_ids)) => user_ids.push(row.user_id),
            None => groups.push((&row.emoji, vec![row.user_id])),
        }
    }
    groups
        .into_iter()
        .map(|(emoji, user_ids)| {
            let reacted = user_ids.contains(&viewer_id);
            let viewer = reacted.then_some(viewer_id);
            let others = user_ids.iter().copied().filter(|id| *id != viewer_id);
            Draft {
                emoji: emoji.to_owned(),
                count: user_ids.len() as u64,
                reacted,
                shown: viewer
                    .into_iter()
                    .chain(others)
                    .take(SUMMARY_USERS_LIMIT)
                    .collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(emoji: &str, user_id: Uuid) -> reactions::Model {
        reactions::Model {
            id: Uuid::new_v4(),
            task_id: Uuid::nil(),
            target_type: TARGET_TASK.into(),
            target_id: Uuid::nil(),
            user_id,
            emoji: emoji.into(),
            created_at: chrono::Utc::now().into(),
        }
    }

    #[test]
    fn keeps_first_reacted_order_of_emoji() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let rows = [row("🎉", a), row("👍", a), row("🎉", b)];
        let emojis: Vec<String> = aggregate(rows.iter(), a)
            .into_iter()
            .map(|d| d.emoji)
            .collect();
        assert_eq!(emojis, ["🎉", "👍"]);
    }

    #[test]
    fn puts_viewer_first_and_cuts_at_the_limit() {
        let ids: Vec<Uuid> = (0..SUMMARY_USERS_LIMIT + 2)
            .map(|_| Uuid::new_v4())
            .collect();
        let rows: Vec<_> = ids.iter().map(|id| row("👍", *id)).collect();

        // 本人は切られる位置（最後）にいても先頭に出る
        let viewer = *ids.last().unwrap();
        let [draft] = <[Draft; 1]>::try_from(aggregate(rows.iter(), viewer)).unwrap();
        assert_eq!(draft.count, ids.len() as u64);
        assert!(draft.reacted);
        assert_eq!(draft.shown[0], viewer);
        assert_eq!(draft.shown[1..], ids[..SUMMARY_USERS_LIMIT - 1]);

        // 押していない人が見ると押した順のまま
        let [draft] = <[Draft; 1]>::try_from(aggregate(rows.iter(), Uuid::new_v4())).unwrap();
        assert!(!draft.reacted);
        assert_eq!(draft.shown, ids[..SUMMARY_USERS_LIMIT]);
    }
}

//! タスク種別の定義（`project_task_types`）まわりの共通処理。
//!
//! # ロックの方針
//!
//! 種別の定義を書く 4 操作（作成・更新・並べ替え・削除）は、トランザクションの最初の文で
//! [`lock_definitions`] を呼ぶ。プロジェクト単位の advisory lock で、何も持たずに待つので、ほかのロックと
//! 輪にならない。4 操作は、これ以外の明示ロック（プロジェクト行・種別の行）を取らない。
//!
//! タスクに種別を付ける側（作成・更新）は、`milestone_id` と同じく、ロックなしで [`find_in_project`] で
//! 確かめて書くだけにする。確認のあとに種別が削除されたら FK が書き込みを弾くので、[`assignment_error`] で
//! `NotFound` にする。削除は 1 文で、付いていたタスクは FK の `ON DELETE SET NULL` で未設定に戻る。
//!
//! 既定の種別の補完（SQL）とプロジェクト作成時の投入はロックを取らない。補完は `ON CONFLICT` と挿入順の固定で
//! 並行実行に耐える。

use std::collections::HashSet;

use sea_orm::sea_query::{CaseStatement, Expr};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DbErr, EntityTrait,
    QueryFilter, SqlErr, prelude::Uuid,
};

use crate::error::AppError;
use entity::project_task_types;

/// 新しいプロジェクトに入れる既定の種別（名前, 色）。並びがそのまま `position` になる。
/// 既存プロジェクトへの追加は `apps/backend/sql/backfill_default_task_types.sql` が同じ値で行う。
pub const DEFAULT_TASK_TYPES: [(&str, &str); 3] = [
    ("タスク", "#64748b"),
    ("バグ", "#ef4444"),
    ("機能", "#3b82f6"),
];

/// プロジェクトに既定の種別を入れる。プロジェクト作成と同じトランザクションで呼ぶ。
pub async fn seed_default_task_types<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<(), AppError> {
    for (position, (name, color)) in DEFAULT_TASK_TYPES.iter().enumerate() {
        project_task_types::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(project_id),
            name: Set((*name).to_string()),
            color: Set((*color).to_string()),
            position: Set(position as i16),
            created_at: Set(chrono::Utc::now().into()),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

/// 同じプロジェクトの種別を返す（ロックなしの読み取り）。無い、または別プロジェクトの種別なら `NotFound`。
pub async fn find_in_project<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    id: Uuid,
) -> Result<project_task_types::Model, AppError> {
    project_task_types::Entity::find_by_id(id)
        .filter(project_task_types::Column::ProjectId.eq(project_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

/// 種別の定義（作成・更新・並べ替え・削除）の書き込みを、プロジェクト単位で直列にする。
///
/// トランザクションの**最初の文**として呼ぶ。キーは 2 引数の形で、他の機能の 1 引数の advisory lock と
/// キー空間が別になる。
pub async fn lock_definitions<C: ConnectionTrait>(
    txn: &C,
    project_id: Uuid,
) -> Result<(), AppError> {
    common::db::execute_bound(
        txn,
        "SELECT pg_advisory_xact_lock(509, hashtext(?))",
        vec![project_id.to_string().into()],
    )
    .await?;
    Ok(())
}

/// タスクの挿入・更新の失敗を `AppError` にする。種別を確かめたあとに削除され、FK が書き込みを弾いたら
/// `NotFound`（削除が先に終わっていた場合と同じ結果）。
pub fn assignment_error(err: DbErr) -> AppError {
    match err.sql_err() {
        Some(SqlErr::ForeignKeyConstraintViolation(message))
            if message.contains("task_type_id") =>
        {
            AppError::NotFound
        }
        _ => err.into(),
    }
}

/// 末尾に足す種別の `position`。現在の最大値（種別が無ければ `None`）の次を返す。
///
/// 種別は `position` を外から渡せず、並べ替えが 0 から振り直す。`SMALLINT` の上限（32,767）に達していたら
/// 400 にする。上限で張り付くと、同じ値の種別が並び（`position`、`id` の順）の末尾に付く保証が無くなる
/// （並べ替えの `position as i16` もこれで収まる）。
pub fn next_position(current_max: Option<i16>) -> Result<i16, AppError> {
    match current_max {
        None => Ok(0),
        Some(max) => max.checked_add(1).ok_or(AppError::BadRequest),
    }
}

/// 並べ替えの `ids` が、既存の種別の集合と過不足なく一致するか。件数の違い・重複・別プロジェクトの ID は 400。
pub fn validate_reorder(
    ids: &[Uuid],
    existing: impl IntoIterator<Item = Uuid>,
) -> Result<(), AppError> {
    let existing: HashSet<Uuid> = existing.into_iter().collect();
    let distinct: HashSet<&Uuid> = ids.iter().collect();
    if ids.len() != existing.len()
        || distinct.len() != ids.len()
        || ids.iter().any(|id| !existing.contains(id))
    {
        return Err(AppError::BadRequest);
    }
    Ok(())
}

/// `ids` の順に、プロジェクトの全種別の `position` を 0 から振り直す（1 回の UPDATE）。
///
/// `(project_id, position)` に UNIQUE は無いので、全件を一度に振り直しても途中で衝突しない。
/// `ids` が空なら何もしない（WHEN の無い CASE は構文エラーになる）。呼び出し側は `validate_reorder` を先に通す。
pub async fn reassign_positions<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    ids: &[Uuid],
) -> Result<(), AppError> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut position_case = CaseStatement::new();
    for (position, id) in ids.iter().enumerate() {
        position_case = position_case.case(
            project_task_types::Column::Id.eq(*id),
            Expr::value(position as i16),
        );
    }
    project_task_types::Entity::update_many()
        .col_expr(
            project_task_types::Column::Position,
            position_case
                .finally(Expr::col(project_task_types::Column::Position))
                .into(),
        )
        .filter(project_task_types::Column::ProjectId.eq(project_id))
        .exec(db)
        .await?;
    Ok(())
}

/// 種別の名前を返す（アクティビティの記録用）。`None` は未設定を表す。
/// 種別が見つからないときも `None` にする（呼び出し側は直前に `find_in_project` で確かめている）。
pub async fn task_type_name<C: ConnectionTrait>(
    db: &C,
    id: Option<Uuid>,
) -> Result<Option<String>, AppError> {
    let Some(id) = id else {
        return Ok(None);
    };
    Ok(project_task_types::Entity::find_by_id(id)
        .one(db)
        .await?
        .map(|t| t.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_position_refuses_to_go_past_the_column_limit() {
        assert_eq!(next_position(None).unwrap(), 0);
        assert_eq!(next_position(Some(4)).unwrap(), 5);
        assert_eq!(next_position(Some(i16::MAX - 1)).unwrap(), i16::MAX);
        assert!(matches!(
            next_position(Some(i16::MAX)),
            Err(AppError::BadRequest)
        ));
    }

    #[test]
    fn reorder_ids_must_match_the_existing_set_exactly() {
        let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        assert!(validate_reorder(&[b, a], [a, b]).is_ok());
        // 0 件の並べ替えは、0 件のときだけ通る
        assert!(validate_reorder(&[], []).is_ok());
        assert!(validate_reorder(&[], [a]).is_err());
        assert!(validate_reorder(&[a], []).is_err());
        assert!(validate_reorder(&[a], [a, b]).is_err());
        assert!(validate_reorder(&[a, a], [a, b]).is_err());
        assert!(validate_reorder(&[a, c], [a, b]).is_err());
    }
}

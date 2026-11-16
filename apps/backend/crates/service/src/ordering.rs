//! プロジェクト単位の定義（種別・カスタムフィールド・ステータス）の並び順（`position`）の採番と振り直し。
//!
//! 並び順を変える書き込みは、プロジェクト行を `FOR UPDATE` で取ってから行い、同じプロジェクトの
//! 書き込みを直列にする。作成が最大値を読んでから挿入するだけだと、同時の作成が同じ値を取る。

use std::collections::HashSet;

use sea_orm::sea_query::{CaseStatement, Expr, LockType};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, prelude::Uuid};

use crate::error::AppError;
use entity::projects;

/// プロジェクト行を `FOR UPDATE` で取る。トランザクションの中で、最大値の読み取りや並び順の検査より先に呼ぶ
/// （ロックの順は常に「プロジェクト行 → 定義の行」）。プロジェクトが無ければ `NotFound`。
pub async fn lock_project<C: ConnectionTrait>(db: &C, project_id: Uuid) -> Result<(), AppError> {
    projects::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(())
}

/// 末尾に足す定義の `position`。現在の最大値（定義が無ければ `None`）の次を返す。
/// `SMALLINT` を越える件数は、値が重なる・負になるのを避けて 400 にする。
pub fn next_position(current_max: Option<i16>) -> Result<i16, AppError> {
    match current_max {
        None => Ok(0),
        Some(max) => max.checked_add(1).ok_or(AppError::BadRequest),
    }
}

/// 並べ替えの `ids` が、既存の定義の集合と過不足なく一致するか。件数の違い・重複・別プロジェクトの ID は 400。
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

/// `ids` の順に、プロジェクトの全定義の `position` を 0 から振り直す（1 回の UPDATE）。
///
/// `(project_id, position)` に UNIQUE は無いので、全件を一度に振り直しても途中で衝突しない。
/// `ids` が空なら何もしない（WHEN の無い CASE は構文エラーになる）。呼び出し側は `validate_reorder` を先に通す。
pub async fn reassign_positions<E, C>(
    db: &C,
    id_column: E::Column,
    position_column: E::Column,
    project_column: E::Column,
    project_id: Uuid,
    ids: &[Uuid],
) -> Result<(), AppError>
where
    E: EntityTrait,
    C: ConnectionTrait,
{
    if ids.is_empty() {
        return Ok(());
    }
    let mut position_case = CaseStatement::new();
    for (position, id) in ids.iter().enumerate() {
        position_case = position_case.case(id_column.eq(*id), Expr::value(position as i16));
    }
    E::update_many()
        .col_expr(
            position_column,
            position_case.finally(Expr::col(position_column)).into(),
        )
        .filter(project_column.eq(project_id))
        .exec(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_position_starts_at_zero_and_stops_at_the_column_limit() {
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

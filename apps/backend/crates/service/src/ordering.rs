//! プロジェクト単位の定義（種別・カスタムフィールド・ステータス）の並び順（`position`）の採番と振り直し。
//!
//! 並び順を変える書き込みは、プロジェクト行を `FOR UPDATE` で取ってから行い、同じプロジェクトの
//! 書き込みを直列にする。作成が最大値を読んでから挿入するだけだと、同時の作成が同じ値を取る。
//!
//! # ロックの方針（種別の行に触る全経路で共通）
//!
//! 種別の操作は、プロジェクト行を `FOR UPDATE` で握らない。タスクを更新する経路は、タスク行を
//! `FOR UPDATE` で取ってから通知などの行を挿入し、その FK の検査がプロジェクト行を `FOR KEY SHARE` で取る。
//! 種別の削除は `SET NULL` でタスク行を待つので、プロジェクト行の `FOR UPDATE` を握ったまま待つと、
//! 「タスク行 → プロジェクト行」の更新と循環する。
//!
//! - 種別の作成・並べ替え、カスタムフィールドの作成、既定の種別の補完（マイグレーションの SQL）:
//!   プロジェクト行を `FOR NO KEY UPDATE`（[`lock_project_definitions`]）。同じ操作同士は直列になるが、
//!   FK の `FOR KEY SHARE` とは衝突しない。並べ替えは続けて種別の全行を `FOR NO KEY UPDATE`
//! - 種別の更新（名前・色）・削除: プロジェクト行を取らない。種別の行だけ（削除は `SET NULL` でタスク行へ進む）
//! - タスクの作成・更新で種別を付ける: 種別の行を `FOR KEY SHARE`（`task_types::lock_for_assignment`）。
//!   更新ではタスク行を取る前に取る（削除が種別の行 → タスク行の順で取るため）。一括作成はカウンター・スプリントの後に
//!   取るが、`FOR KEY SHARE` 同士は待たず、種別を管理する側はカウンター・スプリントを取らないので循環しない
//! - ステータスの作成・更新・削除・並べ替えは、従来どおりプロジェクト行を `FOR UPDATE`（[`lock_project`]）で取る。
//!   種別の行には触らない
//!
//! 逆順になる組が無いことは、第 4 巡の修正記録のロックの表で確かめた。ロックの持ち主はこのモジュールと
//! `task_types::lock_for_assignment` だけにする。ハンドラーで種別の行を直接ロックしない（並べ替えを除く）。

use std::collections::HashSet;

use sea_orm::sea_query::{CaseStatement, Expr, LockType};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, prelude::Uuid};

use crate::error::AppError;
use entity::projects;

/// プロジェクト行を `FOR UPDATE` で取る（ステータスの並べ替え。従来どおり）。種別・カスタムフィールドは
/// FK の `FOR KEY SHARE` と衝突しない [`lock_project_definitions`] を使う。プロジェクトが無ければ `NotFound`。
pub async fn lock_project<C: ConnectionTrait>(db: &C, project_id: Uuid) -> Result<(), AppError> {
    projects::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(())
}

/// プロジェクト行を `FOR NO KEY UPDATE` で取り、同じプロジェクトの定義（種別・カスタムフィールド）の
/// 作成・並べ替えを直列にする。トランザクションの中で、最大値の読み取りや並び順の検査より先に呼ぶ。
/// タスクや通知の挿入が FK の検査で取る `FOR KEY SHARE` とは衝突しない。プロジェクトが無ければ `NotFound`。
pub async fn lock_project_definitions<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<(), AppError> {
    projects::Entity::find_by_id(project_id)
        .lock(LockType::NoKeyUpdate)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(())
}

/// 末尾に足す定義の `position`。現在の最大値（定義が無ければ `None`）の次を返す。
///
/// `SMALLINT` の上限（32,767）では張り付き、作成は失敗させない。カスタムフィールドは作成と更新で
/// `position` を自由に渡せるので、上限の値を 1 件持つだけで以後の作成が全部失敗するのを避ける
/// （重なった値は、更新で `position` を渡して直せる）。
/// 種別は [`next_unique_position`] を使う。ステータスは作成で位置を受け取り、この関数を使わない。
pub fn next_position(current_max: Option<i16>) -> i16 {
    current_max.map_or(0, |max| max.saturating_add(1))
}

/// 末尾に足す定義の `position`。[`next_position`] と違い、上限（32,767）に達していたら 400 にする。
///
/// 種別は `position` を外から渡せず、並べ替えが 0 から振り直す。上限で張り付くと、同じ値の種別が
/// 並び（`position`、`id` の順）で末尾に付く保証が無くなるので、作成を断って件数を 32,768 以下に保つ
/// （並べ替えの `position as i16` もこれで収まる）。
pub fn next_unique_position(current_max: Option<i16>) -> Result<i16, AppError> {
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
    fn next_position_starts_at_zero_and_sticks_at_the_column_limit() {
        assert_eq!(next_position(None), 0);
        assert_eq!(next_position(Some(4)), 5);
        assert_eq!(next_position(Some(i16::MAX - 1)), i16::MAX);
        // 上限に達しても失敗せず、上限の値のまま
        assert_eq!(next_position(Some(i16::MAX)), i16::MAX);
    }

    #[test]
    fn next_unique_position_refuses_to_go_past_the_column_limit() {
        assert_eq!(next_unique_position(None).unwrap(), 0);
        assert_eq!(next_unique_position(Some(4)).unwrap(), 5);
        assert_eq!(next_unique_position(Some(i16::MAX - 1)).unwrap(), i16::MAX);
        assert!(matches!(
            next_unique_position(Some(i16::MAX)),
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

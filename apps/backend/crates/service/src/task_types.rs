//! タスク種別の定義（`project_task_types`）まわりの共通処理。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect, prelude::Uuid, sea_query::LockType,
};

use crate::error::AppError;
use entity::{project_task_types, projects};

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

/// タスクに付ける種別を確かめ、プロジェクト行と種別の行を `FOR KEY SHARE` で取る。
///
/// タスクの作成・更新のトランザクションの中で、**タスク行を取る前**に呼ぶ（ロックの順は
/// [`crate::ordering`] の説明のとおり）。確認からコミットまで種別の削除・並べ替えを待たせ、FK 違反（500）に
/// ならずに `SET NULL` で外れる。種別の削除はプロジェクト行 → 種別の行 → タスク行の順に取るので、
/// 逆順（タスク行 → 種別の行）で取るとデッドロックになる。無い、または別プロジェクトの種別なら `NotFound`。
pub async fn lock_for_assignment<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    id: Uuid,
) -> Result<project_task_types::Model, AppError> {
    projects::Entity::find_by_id(project_id)
        .lock(LockType::KeyShare)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    project_task_types::Entity::find_by_id(id)
        .filter(project_task_types::Column::ProjectId.eq(project_id))
        .lock(LockType::KeyShare)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
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

//! タスクの添付一覧。添付一覧 API とタスク詳細の応答で共有する。

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, prelude::Uuid};

use crate::error::AppError;
use entity::{drive_files, task_attachments};
use payload::task_extensions::TaskAttachmentResponse;

/// タスクの添付を新しい順に返す（Drive ファイルの行との JOIN 1 回）。
pub async fn list_for_task<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
) -> Result<Vec<TaskAttachmentResponse>, AppError> {
    task_attachments::Entity::find()
        .filter(task_attachments::Column::TaskId.eq(task_id))
        .order_by_desc(task_attachments::Column::CreatedAt)
        .find_also_related(drive_files::Entity)
        .all(db)
        .await?
        .into_iter()
        .map(|(row, file)| {
            let file = file.ok_or(AppError::NotFound)?;
            Ok(TaskAttachmentResponse::from_parts(row, file))
        })
        .collect()
}

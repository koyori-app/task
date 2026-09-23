use sea_orm_migration::prelude::*;

/// 通知の拡張（`docs/features/tasks/5.notifications.md`）。
///
/// - `project_id`: 一覧の認可をタスク経由ではなくプロジェクトで絞るための列。
///   既存行は `tasks.project_id` から埋める。`task_id` の無い行はこれまで生成されて
///   おらず、プロジェクトを決められないので消してから NOT NULL にする
/// - `target`: 遷移先。既存行は全部タスク通知なので `{"type":"task","task_id":…}` で埋める
/// - `dedupe_key`: 冪等化キー。列の UNIQUE にする（部分インデックスは起動時の
///   schema sync が DROP CONSTRAINT で消そうとして落ちる。NULL 同士は重複扱いされない）
/// - `(user_id, created_at DESC, id DESC)`: カーソルの並び順の索引
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
ALTER TABLE notifications ADD COLUMN project_id UUID;
UPDATE notifications n SET project_id = t.project_id FROM tasks t WHERE n.task_id = t.id;
DELETE FROM notifications WHERE project_id IS NULL;
ALTER TABLE notifications ALTER COLUMN project_id SET NOT NULL;

ALTER TABLE notifications ADD COLUMN target JSONB;
UPDATE notifications SET target = jsonb_build_object('type', 'task', 'task_id', task_id);
ALTER TABLE notifications ALTER COLUMN target SET NOT NULL;

ALTER TABLE notifications ADD COLUMN dedupe_key VARCHAR;
ALTER TABLE notifications
    ADD CONSTRAINT notifications_user_id_dedupe_key_key UNIQUE (user_id, dedupe_key);

CREATE INDEX idx_notifications_user_created_id
    ON notifications (user_id, created_at DESC, id DESC);
"#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
DROP INDEX IF EXISTS idx_notifications_user_created_id;
ALTER TABLE notifications DROP CONSTRAINT IF EXISTS notifications_user_id_dedupe_key_key;
ALTER TABLE notifications DROP COLUMN IF EXISTS dedupe_key;
ALTER TABLE notifications DROP COLUMN IF EXISTS target;
ALTER TABLE notifications DROP COLUMN IF EXISTS project_id;
"#,
            )
            .await?;
        Ok(())
    }
}

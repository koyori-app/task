use sea_orm_migration::prelude::*;

/// 既定の種別（タスク / バグ / 機能）を、種別が 1 件も無いプロジェクトへ入れる SQL。
/// 統合テストと共有するため `apps/backend/sql/` に置く。
const BACKFILL_SQL: &str = include_str!("../../sql/backfill_default_task_types.sql");

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// プロジェクト単位のタスク種別の定義表と、`tasks.task_type_id` を足す。
    /// 新しいバックエンドをマイグレーションより先に起動すると SeaORM の `sync()` が表と列を作るので、
    /// `IF NOT EXISTS` で受ける。一意性は表の制約で持ち、UNIQUE インデックスは足さない。
    /// 種別の削除は `ON DELETE SET NULL` でタスクの種別を未設定へ戻す。
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS project_task_types (
                id UUID PRIMARY KEY,
                project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                name VARCHAR(100) NOT NULL,
                color VARCHAR(7) NOT NULL,
                position SMALLINT NOT NULL DEFAULT 0,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (project_id, name)
            )",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS task_type_id UUID
                REFERENCES project_task_types(id) ON DELETE SET NULL",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_tasks_task_type_id ON tasks(task_type_id)",
        )
        .await?;
        conn.execute_unprepared(BACKFILL_SQL).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared("ALTER TABLE tasks DROP COLUMN IF EXISTS task_type_id")
            .await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS project_task_types")
            .await?;
        Ok(())
    }
}

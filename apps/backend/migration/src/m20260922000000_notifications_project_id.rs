use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 通知の可視性はプロジェクトで判定する。レビュー通知は PR 単位でタスクに
        // 紐づかないので、task_id 経由では「見せてよい相手か」を決められない
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE notifications
                 ADD COLUMN project_id UUID REFERENCES projects(id) ON DELETE CASCADE",
            )
            .await?;
        // 既存行はタスクのプロジェクトを埋める（埋めないと判定が全件 NULL 扱いになる）
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE notifications n SET project_id = t.project_id
                 FROM tasks t WHERE n.task_id = t.id",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared("CREATE INDEX idx_notifications_project ON notifications(project_id)")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_notifications_project")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE notifications DROP COLUMN IF EXISTS project_id")
            .await?;
        Ok(())
    }
}

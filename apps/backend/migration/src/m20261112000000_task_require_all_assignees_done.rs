use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 件数は 1 タスクの担当者数で小さいので、インデックスや UNIQUE は足さない
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS require_all_assignees_done BOOLEAN NOT NULL DEFAULT false",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE task_assignees ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE task_assignees DROP COLUMN IF EXISTS completed_at")
            .await?;
        db.execute_unprepared("ALTER TABLE tasks DROP COLUMN IF EXISTS require_all_assignees_done")
            .await?;
        Ok(())
    }
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 添付の由来。manual = 手動添付、body = Drive の Markdown 本文から抽出した参照。
        // 既存行はすべて手動で付けたものなので manual で埋まる
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE task_attachments ADD COLUMN IF NOT EXISTS source VARCHAR(16) NOT NULL DEFAULT 'manual'",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE task_attachments DROP COLUMN IF EXISTS source")
            .await?;
        Ok(())
    }
}

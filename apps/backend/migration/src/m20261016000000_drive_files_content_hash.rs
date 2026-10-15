use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 本文の SHA-256（hex）。ETag に使う。既存行は NULL のまま残し、
        // ETag は updated_at とサイズで代替する（backfill はしない）
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE drive_files ADD COLUMN IF NOT EXISTS content_hash VARCHAR(64)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE drive_files DROP COLUMN IF EXISTS content_hash")
            .await?;
        Ok(())
    }
}

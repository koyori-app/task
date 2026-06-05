
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "ALTER TABLE tenants ADD COLUMN IF NOT EXISTS drive_quota_bytes BIGINT";
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "ALTER TABLE tenants DROP COLUMN IF EXISTS drive_quota_bytes";
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

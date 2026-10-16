use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // アップロードしたアバターのストレージキー。外部 URL のアバターでは NULL
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar_storage_key TEXT")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE users DROP COLUMN IF EXISTS avatar_storage_key")
            .await?;
        Ok(())
    }
}

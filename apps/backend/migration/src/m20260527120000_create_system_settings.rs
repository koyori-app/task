use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS system_settings (
                    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton = true),
                    user_registration_enabled BOOLEAN NOT NULL DEFAULT true,
                    drive_default_quota_mb BIGINT NOT NULL DEFAULT 10240,
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
                );
                INSERT INTO system_settings DEFAULT VALUES ON CONFLICT DO NOTHING;",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS system_settings;")
            .await?;
        Ok(())
    }
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 計画用の日付なので soft_deadline との前後関係は CHECK で縛らない
        // （締切は timestamptz で、タイムゾーン次第で正しい組を弾いてしまう）
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS planned_start_on DATE")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE tasks DROP COLUMN IF EXISTS planned_start_on")
            .await?;
        Ok(())
    }
}

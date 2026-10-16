use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// 取り込み元 Issue の開閉状態（GitHub 側の値）を控える。
/// タスクのステータスからは推定しない（完了ステータスとの対応は同期時にしか成り立たない）ので、
/// 同期で控えていない既存行は `unknown` のままにする。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE github_issue_links \
                 ADD COLUMN IF NOT EXISTS github_state VARCHAR NOT NULL DEFAULT 'unknown'",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE github_issue_links DROP COLUMN IF EXISTS github_state")
            .await?;
        Ok(())
    }
}

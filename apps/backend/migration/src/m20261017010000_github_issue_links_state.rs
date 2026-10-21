use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// 取り込み元 Issue の開閉状態（GitHub 側の値）を控える。
/// タスクのステータスからは推定しない（完了ステータスとの対応は同期時にしか成り立たない）ので、
/// 同期で控えていない既存行は `unknown` のままにする。
///
/// 最後に同期（取り込み・書き戻し）した時刻も控える。`updated_at` は書き戻し要求でも進む版なので
/// 同期の時刻には使えない。既存行の同期の時刻は分からないので NULL のままにする。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE github_issue_links \
                 ADD COLUMN IF NOT EXISTS github_state VARCHAR NOT NULL DEFAULT 'unknown', \
                 ADD COLUMN IF NOT EXISTS last_synced_at TIMESTAMPTZ",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE github_issue_links \
                 DROP COLUMN IF EXISTS github_state, \
                 DROP COLUMN IF EXISTS last_synced_at",
            )
            .await?;
        Ok(())
    }
}

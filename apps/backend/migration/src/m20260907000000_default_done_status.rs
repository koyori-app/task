use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 完了ステータスを複数持てるようにしたので、「完了にする」操作で使う 1 つを
        // 印で選べるようにする。これまでは完了ステータスがプロジェクトに 1 つだけ
        // だったので、既存の完了ステータスをそのまま既定にする。
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE project_statuses
                 ADD COLUMN IF NOT EXISTS is_default_done BOOLEAN NOT NULL DEFAULT false",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE project_statuses SET is_default_done = true WHERE is_done_state = true",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE project_statuses DROP COLUMN IF EXISTS is_default_done",
            )
            .await?;
        Ok(())
    }
}

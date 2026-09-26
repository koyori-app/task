use sea_orm_migration::prelude::*;

/// 通知一覧のカーソル（`created_at DESC, id DESC`）の並び順の索引
/// （`docs/features/tasks/5.notifications.md`）。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_notifications_user_created_id \
                 ON notifications (user_id, created_at DESC, id DESC)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_notifications_user_created_id")
            .await?;
        Ok(())
    }
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // メール送信の待ち行列を通知の行そのものに持つ（outbox）。通知を作るのは
        // service::notifications 1 箇所で、そこから apalis へは積めない（依存の向き）
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE notifications
                 ADD COLUMN email_queued_at TIMESTAMPTZ,
                 ADD COLUMN emailed_at TIMESTAMPTZ,
                 ADD COLUMN email_attempts SMALLINT NOT NULL DEFAULT 0",
            )
            .await?;
        // 掃き出しループの取得条件そのもの。送信済み・非対象の行を走査させない
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_notifications_email_pending ON notifications(created_at)
                 WHERE email_queued_at IS NOT NULL AND emailed_at IS NULL",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_notifications_email_pending")
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE notifications
                 DROP COLUMN IF EXISTS email_queued_at,
                 DROP COLUMN IF EXISTS emailed_at,
                 DROP COLUMN IF EXISTS email_attempts",
            )
            .await?;
        Ok(())
    }
}

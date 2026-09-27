use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 外部向け Webhook（docs/features/tasks/10.webhooks.md）。
        // secret は平文で持たない（auth_core::crypto::encrypt_token で暗号化した値）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE webhooks (
                    id UUID PRIMARY KEY,
                    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    url VARCHAR(2048) NOT NULL,
                    secret_enc VARCHAR NOT NULL,
                    events VARCHAR[] NOT NULL,
                    format VARCHAR NOT NULL DEFAULT 'json',
                    is_active BOOLEAN NOT NULL DEFAULT true,
                    failure_streak SMALLINT NOT NULL DEFAULT 0,
                    created_by UUID NOT NULL REFERENCES users(id),
                    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                    CONSTRAINT chk_webhooks_format CHECK (format IN ('json', 'discord'))
                )",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared("CREATE INDEX idx_webhooks_project ON webhooks(project_id)")
            .await?;
        // 配信の行そのものが送信待ちの行列（outbox）。next_attempt_at が NULL なら完了
        // （成功または打ち止め）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE webhook_deliveries (
                    id UUID PRIMARY KEY,
                    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
                    event VARCHAR NOT NULL,
                    payload JSONB NOT NULL,
                    status_code INT,
                    attempt SMALLINT NOT NULL DEFAULT 0,
                    next_attempt_at TIMESTAMPTZ,
                    last_error TEXT,
                    delivered_at TIMESTAMPTZ,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_webhook_deliveries_webhook
                 ON webhook_deliveries(webhook_id, created_at DESC)",
            )
            .await?;
        // 掃き出しループの取得条件。完了済みの行を走査させない
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_webhook_deliveries_pending ON webhook_deliveries(next_attempt_at)
                 WHERE next_attempt_at IS NOT NULL",
            )
            .await?;
        // 90 日より古い履歴の掃除用
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_webhook_deliveries_created ON webhook_deliveries(created_at)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS webhook_deliveries")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS webhooks")
            .await?;
        Ok(())
    }
}

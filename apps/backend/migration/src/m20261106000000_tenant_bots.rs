use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE users
                ADD COLUMN account_kind text NOT NULL DEFAULT 'human',
                ADD COLUMN bot_tenant_id uuid REFERENCES tenants(id) ON DELETE CASCADE,
                ADD CONSTRAINT users_account_kind_check CHECK (
                    (account_kind = 'human' AND bot_tenant_id IS NULL)
                    OR (account_kind = 'bot' AND bot_tenant_id IS NOT NULL
                        AND email = id::text || '@bots.task.invalid'
                        AND password_hash IS NULL AND NOT totp_enabled AND NOT is_admin)
                );
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 旧版では BOT の区別がないため、鍵を取り消して停止状態で履歴を保つ。
        manager.get_connection().execute_unprepared(r#"
            UPDATE personal_tokens SET revoked = true WHERE user_id IN (SELECT id FROM users WHERE account_kind = 'bot');
            UPDATE users SET is_suspended = true, sessions_revoked_at = NOW() WHERE account_kind = 'bot';
            ALTER TABLE users DROP CONSTRAINT users_account_kind_check,
                DROP COLUMN bot_tenant_id, DROP COLUMN account_kind;
        "#).await?;
        Ok(())
    }
}

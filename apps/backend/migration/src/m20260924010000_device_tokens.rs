use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Koyori Desktop の Bearer 資格情報。PAT と違いテナントに束縛せず、権限は
        // セッション相当（apps/backend/docs/personal-access-tokens-authz.md の Desktop 認証）。
        // 失効は revoked_at を立てるだけで行は残す（端末一覧に失効済みとして出さないだけ）。
        // token_hash は列の UNIQUE で持つ（部分 UNIQUE は起動時の schema sync が落とす）
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE device_tokens (
                id              UUID PRIMARY KEY,
                user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                name            VARCHAR NOT NULL,
                token_hash      VARCHAR NOT NULL UNIQUE,
                token_last_four VARCHAR NOT NULL,
                expires_at      TIMESTAMPTZ NOT NULL,
                last_used_at    TIMESTAMPTZ,
                revoked_at      TIMESTAMPTZ,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
            )
        "#,
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared("CREATE INDEX idx_device_tokens_user_id ON device_tokens(user_id)")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS device_tokens")
            .await?;
        Ok(())
    }
}

use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS passkeys (
                id UUID PRIMARY KEY,
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                credential_id BYTEA NOT NULL UNIQUE,
                public_key BYTEA NOT NULL,
                aaguid BYTEA,
                sign_count BIGINT NOT NULL DEFAULT 0,
                name VARCHAR(255) NOT NULL,
                last_used_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );

            CREATE INDEX IF NOT EXISTS idx_passkeys_user ON passkeys(user_id);
        "#;
        manager
            .get_connection()
            .execute_unprepared(sql)
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS passkeys")
            .await
            .map(|_| ())
    }
}

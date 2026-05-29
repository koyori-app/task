use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
ALTER TABLE users ADD COLUMN IF NOT EXISTS totp_enabled BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS require_2fa BOOLEAN NOT NULL DEFAULT false;

CREATE TABLE IF NOT EXISTS totp_credentials (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_enc TEXT NOT NULL,
    is_verified BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS recovery_codes (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash VARCHAR NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_recovery_codes_user ON recovery_codes(user_id);
"#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
DROP INDEX IF EXISTS idx_recovery_codes_user;
DROP TABLE IF EXISTS recovery_codes;
DROP TABLE IF EXISTS totp_credentials;
ALTER TABLE tenants DROP COLUMN IF EXISTS require_2fa;
ALTER TABLE users DROP COLUMN IF EXISTS totp_enabled;
"#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }
}

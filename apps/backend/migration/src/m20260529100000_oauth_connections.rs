use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let alter_users = r#"
            ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                alter_users.to_owned(),
            ))
            .await?;

        let create_oauth_connections = r#"
            CREATE TABLE IF NOT EXISTS oauth_connections (
                id UUID PRIMARY KEY,
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                provider VARCHAR NOT NULL,
                provider_user_id VARCHAR NOT NULL,
                provider_email VARCHAR,
                instance_url VARCHAR,
                access_token_enc TEXT,
                refresh_token_enc TEXT,
                token_expires_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE NULLS NOT DISTINCT (provider, provider_user_id, instance_url)
            )
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                create_oauth_connections.to_owned(),
            ))
            .await?;

        let create_index = r#"
            CREATE INDEX IF NOT EXISTS idx_oauth_connections_user ON oauth_connections(user_id)
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                create_index.to_owned(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rollback order: drop oauth_connections first, then restore password_hash NOT NULL.
        // OAuth-only users have NULL password_hash; fill placeholders before SET NOT NULL.
        let drop_index = "DROP INDEX IF EXISTS idx_oauth_connections_user";
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                drop_index.to_owned(),
            ))
            .await?;

        let drop_table = "DROP TABLE IF EXISTS oauth_connections";
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                drop_table.to_owned(),
            ))
            .await?;

        // OAuth-only users may have NULL password_hash; set a placeholder before NOT NULL.
        let fill_null_passwords = r#"
            UPDATE users
            SET password_hash = 'oauth-down-migration-placeholder'
            WHERE password_hash IS NULL
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                fill_null_passwords.to_owned(),
            ))
            .await?;

        let alter_users = r#"
            ALTER TABLE users ALTER COLUMN password_hash SET NOT NULL
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                alter_users.to_owned(),
            ))
            .await?;

        Ok(())
    }
}

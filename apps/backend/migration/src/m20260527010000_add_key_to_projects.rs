use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE projects
                ADD COLUMN IF NOT EXISTS key VARCHAR(10) NOT NULL DEFAULT '',
                ADD CONSTRAINT IF NOT EXISTS projects_key_format CHECK (key ~ '^[A-Z][A-Z0-9]{1,9}$'),
                ADD CONSTRAINT IF NOT EXISTS projects_key_tenant_unique UNIQUE (tenant_id, key)
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE projects
                DROP CONSTRAINT IF EXISTS projects_key_tenant_unique,
                DROP CONSTRAINT IF EXISTS projects_key_format,
                DROP COLUMN IF EXISTS key
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }
}

use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS drive_files (
                id UUID PRIMARY KEY,
                name VARCHAR NOT NULL,
                size BIGINT NOT NULL,
                mime_type VARCHAR NOT NULL,
                storage_type VARCHAR(16) NOT NULL,
                storage_key VARCHAR NOT NULL,
                tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
                uploader_id UUID NOT NULL REFERENCES users(id) ON DELETE NO ACTION,
                folder_id UUID REFERENCES drive_folders(id) ON DELETE SET NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT drive_files_project_folder_check
                    CHECK (project_id IS NULL OR folder_id IS NOT NULL)
            )
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "DROP TABLE IF EXISTS drive_files";
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }
}


use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS drive_folders (
                id UUID PRIMARY KEY,
                name VARCHAR NOT NULL,
                parent_id UUID REFERENCES drive_folders(id) ON DELETE SET NULL,
                tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
                created_by UUID NOT NULL REFERENCES users(id) ON DELETE NO ACTION,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "DROP TABLE IF EXISTS drive_folders";
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

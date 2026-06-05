
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE labels ADD COLUMN IF NOT EXISTS project_id UUID REFERENCES projects(id) ON DELETE CASCADE;
            ALTER TABLE labels ADD CONSTRAINT IF NOT EXISTS labels_project_name_unique UNIQUE (project_id, name)
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE labels DROP CONSTRAINT IF EXISTS labels_project_name_unique;
            ALTER TABLE labels DROP COLUMN IF EXISTS project_id
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

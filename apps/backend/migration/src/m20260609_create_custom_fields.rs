use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS project_custom_fields (
                id UUID PRIMARY KEY,
                project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                name VARCHAR(100) NOT NULL,
                field_type VARCHAR NOT NULL CHECK (field_type IN ('text', 'number', 'select', 'date', 'url', 'checkbox')),
                options JSONB,
                is_required BOOLEAN NOT NULL DEFAULT false,
                position SMALLINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (project_id, name)
            );
            CREATE INDEX IF NOT EXISTS idx_project_custom_fields_project_position 
                ON project_custom_fields(project_id, position);

            CREATE TABLE IF NOT EXISTS task_custom_field_values (
                task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                field_id UUID NOT NULL REFERENCES project_custom_fields(id) ON DELETE CASCADE,
                value TEXT,
                PRIMARY KEY (task_id, field_id)
            );
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            DROP TABLE IF EXISTS task_custom_field_values;
            DROP TABLE IF EXISTS project_custom_fields;
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }
}

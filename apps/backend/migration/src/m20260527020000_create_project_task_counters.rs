
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS project_task_counters (
                project_id UUID PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                last_seq INT NOT NULL DEFAULT 0
            )
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "DROP TABLE IF EXISTS project_task_counters";
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

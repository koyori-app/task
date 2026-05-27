use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS task_relations (
                id UUID PRIMARY KEY,
                blocker_task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                blocked_task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (blocker_task_id, blocked_task_id),
                CHECK (blocker_task_id <> blocked_task_id)
            )
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "DROP TABLE IF EXISTS task_relations";
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }
}

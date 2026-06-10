use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS time_logs (
                id UUID PRIMARY KEY,
                task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                logged_minutes INT NOT NULL CHECK (logged_minutes > 0),
                logged_at DATE NOT NULL,
                note TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );

            CREATE TABLE IF NOT EXISTS task_timers (
                task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (task_id, user_id)
            );

            CREATE INDEX IF NOT EXISTS idx_time_logs_task ON time_logs(task_id);
            CREATE INDEX IF NOT EXISTS idx_time_logs_user_date ON time_logs(user_id, logged_at);
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            DROP TABLE IF EXISTS task_timers;
            DROP TABLE IF EXISTS time_logs;
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }
}

use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id UUID PRIMARY KEY,
                project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                seq_id INT NOT NULL,
                title VARCHAR(255) NOT NULL,
                description TEXT,
                status_id UUID NOT NULL REFERENCES project_statuses(id),
                priority VARCHAR NOT NULL DEFAULT 'medium',
                progress_pct SMALLINT NOT NULL DEFAULT 0 CHECK (progress_pct BETWEEN 0 AND 100),
                parent_task_id UUID REFERENCES tasks(id) ON DELETE SET NULL,
                milestone_id UUID REFERENCES milestones(id) ON DELETE SET NULL,
                soft_deadline TIMESTAMPTZ,
                hard_deadline TIMESTAMPTZ,
                estimated_minutes INT CHECK (estimated_minutes > 0),
                is_archived BOOLEAN NOT NULL DEFAULT false,
                created_by UUID NOT NULL REFERENCES users(id) ON DELETE NO ACTION,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                deleted_at TIMESTAMPTZ,
                UNIQUE (project_id, seq_id),
                CONSTRAINT soft_before_hard CHECK (
                    soft_deadline IS NULL OR hard_deadline IS NULL OR soft_deadline <= hard_deadline
                )
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks(project_id) WHERE deleted_at IS NULL;
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_parent ON tasks(parent_task_id) WHERE parent_task_id IS NOT NULL
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            DROP INDEX IF EXISTS idx_tasks_parent;
            DROP INDEX IF EXISTS idx_tasks_status;
            DROP INDEX IF EXISTS idx_tasks_project;
            DROP TABLE IF EXISTS tasks
        "#;
        let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
        manager.get_connection().execute(stmt).await.map(|_| ())
    }
}

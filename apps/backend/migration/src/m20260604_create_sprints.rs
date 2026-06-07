use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let create_sprints = r#"
            CREATE TABLE IF NOT EXISTS sprints (
                id UUID PRIMARY KEY,
                project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                name VARCHAR(255) NOT NULL,
                goal TEXT,
                start_date DATE NOT NULL,
                end_date DATE NOT NULL,
                status VARCHAR NOT NULL DEFAULT 'planning',
                created_by UUID NOT NULL REFERENCES users(id) ON DELETE NO ACTION,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CHECK (start_date <= end_date),
                CONSTRAINT sprints_status_check
                    CHECK (status IN ('planning', 'active', 'completed'))
            )
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                create_sprints.to_owned(),
            ))
            .await?;

        let project_idx =
            "CREATE INDEX IF NOT EXISTS idx_sprints_project ON sprints(project_id)";
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                project_idx.to_owned(),
            ))
            .await?;

        let add_task_columns = r#"
            ALTER TABLE tasks
                ADD COLUMN IF NOT EXISTS sprint_id UUID REFERENCES sprints(id) ON DELETE SET NULL,
                ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                add_task_columns.to_owned(),
            ))
            .await?;

        let backfill_completed_at = r#"
            UPDATE tasks
            SET completed_at = tasks.updated_at
            FROM project_statuses
            WHERE tasks.status_id = project_statuses.id
              AND project_statuses.is_done_state = TRUE
              AND tasks.completed_at IS NULL
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                backfill_completed_at.to_owned(),
            ))
            .await?;

        let idx = r#"
            CREATE INDEX IF NOT EXISTS idx_tasks_sprint ON tasks(sprint_id) WHERE sprint_id IS NOT NULL
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                idx.to_owned(),
            ))
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let drop_idx = "DROP INDEX IF EXISTS idx_tasks_sprint";
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                drop_idx.to_owned(),
            ))
            .await?;

        let drop_project_idx = "DROP INDEX IF EXISTS idx_sprints_project";
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                drop_project_idx.to_owned(),
            ))
            .await?;

        let drop_columns = r#"
            ALTER TABLE tasks
                DROP COLUMN IF EXISTS completed_at,
                DROP COLUMN IF EXISTS sprint_id
        "#;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                drop_columns.to_owned(),
            ))
            .await?;

        let drop_table = "DROP TABLE IF EXISTS sprints";
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                drop_table.to_owned(),
            ))
            .await
            .map(|_| ())
    }
}

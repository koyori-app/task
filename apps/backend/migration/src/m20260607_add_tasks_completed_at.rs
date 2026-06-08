use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE tasks
                    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ;

                DO $$
                BEGIN
                    IF NOT EXISTS (
                        SELECT 1
                        FROM pg_constraint
                        WHERE conname = 'sprints_status_check'
                          AND conrelid = 'sprints'::regclass
                    ) THEN
                        ALTER TABLE sprints
                            ADD CONSTRAINT sprints_status_check
                            CHECK (status IN ('planning', 'active', 'completed'));
                    END IF;
                END
                $$;

                UPDATE tasks
                SET completed_at = tasks.updated_at
                FROM project_statuses
                WHERE tasks.status_id = project_statuses.id
                  AND project_statuses.is_done_state = TRUE
                  AND tasks.completed_at IS NULL;
                "#,
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE tasks DROP COLUMN IF EXISTS completed_at;
                ALTER TABLE sprints DROP CONSTRAINT IF EXISTS sprints_status_check;
                "#,
            )
            .await
            .map(|_| ())
    }
}


use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_constraint
                    WHERE conname = 'uq_task_assignees_task_user'
                ) THEN
                    ALTER TABLE task_assignees
                        ADD CONSTRAINT uq_task_assignees_task_user
                        UNIQUE (task_id, user_id);
                END IF;
            END $$;
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE task_assignees
                DROP CONSTRAINT IF EXISTS uq_task_assignees_task_user
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

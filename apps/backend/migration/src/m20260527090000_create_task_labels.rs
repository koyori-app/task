
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS task_labels (
                task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
                PRIMARY KEY (task_id, label_id)
            );
            CREATE INDEX IF NOT EXISTS idx_task_labels_label_id ON task_labels (label_id)
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "DROP TABLE IF EXISTS task_labels";
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

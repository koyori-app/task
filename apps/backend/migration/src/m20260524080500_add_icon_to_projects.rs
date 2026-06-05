
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE projects
              ADD COLUMN icon_emoji VARCHAR,
              ADD COLUMN icon_url VARCHAR
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE projects
              DROP COLUMN IF EXISTS icon_emoji,
              DROP COLUMN IF EXISTS icon_url
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

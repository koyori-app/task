
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE project_members
                ADD CONSTRAINT project_members_role_check
                CHECK (role IN ('Admin', 'Member', 'Viewer'))
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE project_members
                DROP CONSTRAINT IF EXISTS project_members_role_check
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

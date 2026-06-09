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
                ALTER TABLE projects
                    ADD COLUMN IF NOT EXISTS is_personal BOOLEAN NOT NULL DEFAULT false,
                    ADD COLUMN IF NOT EXISTS personal_owner_id UUID REFERENCES users(id) ON DELETE CASCADE;

                CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_personal_owner
                    ON projects(tenant_id, personal_owner_id)
                    WHERE is_personal = true;
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
                DROP INDEX IF EXISTS idx_projects_personal_owner;
                ALTER TABLE projects DROP COLUMN IF EXISTS personal_owner_id;
                ALTER TABLE projects DROP COLUMN IF EXISTS is_personal;
                "#,
            )
            .await
            .map(|_| ())
    }
}

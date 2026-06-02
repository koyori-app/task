use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        let steps = [
            r#"
            ALTER TABLE projects
                ADD COLUMN IF NOT EXISTS key VARCHAR(10)
            "#,
            r#"
            UPDATE projects
            SET key = 'P' || upper(substring(replace(id::text, '-', '') from 1 for 3))
            WHERE key IS NULL
            "#,
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_constraint WHERE conname = 'projects_key_format'
                ) THEN
                    ALTER TABLE projects
                        ADD CONSTRAINT projects_key_format
                        CHECK (key ~ '^[A-Z][A-Z0-9]{1,9}$');
                END IF;
            END $$;
            "#,
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS projects_key_tenant_unique
                ON projects (tenant_id, key)
            "#,
            r#"
            ALTER TABLE projects
                ALTER COLUMN key SET NOT NULL
            "#,
        ];
        for sql in steps {
            let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
            conn.execute(stmt).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        let steps = [
            r#"
            ALTER TABLE projects
                ALTER COLUMN key DROP NOT NULL
            "#,
            r#"
            DROP INDEX IF EXISTS projects_key_tenant_unique
            "#,
            r#"
            ALTER TABLE projects
                DROP CONSTRAINT IF EXISTS projects_key_format
            "#,
            r#"
            ALTER TABLE projects
                DROP COLUMN IF EXISTS key
            "#,
        ];
        for sql in steps {
            let stmt = Statement::from_string(manager.get_database_backend(), sql.to_owned());
            conn.execute(stmt).await?;
        }
        Ok(())
    }
}

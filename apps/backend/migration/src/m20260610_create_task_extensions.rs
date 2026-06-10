use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

fn search_ts_config() -> &'static str {
    match std::env::var("USE_PG_BIGM") {
        Ok(v) if v == "1" || v.eq_ignore_ascii_case("true") => "public.pg_bigm",
        _ => "pg_catalog.simple",
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let ts_config = search_ts_config();
        let sql = format!(
            r#"
ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS search_vector tsvector
    GENERATED ALWAYS AS (
        to_tsvector('{ts_config}',
            coalesce(title, '') || ' ' || coalesce(description, ''))
    ) STORED;

CREATE INDEX IF NOT EXISTS idx_tasks_search_vector ON tasks USING GIN(search_vector);

CREATE TABLE IF NOT EXISTS project_task_views (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    is_shared BOOLEAN NOT NULL DEFAULT false,
    filters JSONB NOT NULL DEFAULT '{}',
    sort JSONB NOT NULL DEFAULT '{}',
    view_type VARCHAR NOT NULL DEFAULT 'list',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_project_task_views_project ON project_task_views(project_id);
CREATE INDEX IF NOT EXISTS idx_project_task_views_project_created_by ON project_task_views(project_id, created_by);

CREATE TABLE IF NOT EXISTS task_attachments (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    drive_file_id UUID NOT NULL REFERENCES drive_files(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (task_id, drive_file_id)
);

CREATE INDEX IF NOT EXISTS idx_task_attachments_task ON task_attachments(task_id);
"#
        );
        manager.get_connection().execute_unprepared(&sql).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
DROP TABLE IF EXISTS task_attachments;
DROP TABLE IF EXISTS project_task_views;
DROP INDEX IF EXISTS idx_tasks_search_vector;
ALTER TABLE tasks DROP COLUMN IF EXISTS search_vector;
"#,
            )
            .await?;
        Ok(())
    }
}

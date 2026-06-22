use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

fn use_pg_bigm() -> bool {
    matches!(
        std::env::var("USE_PG_BIGM").as_deref(),
        Ok("1") | Ok("true") | Ok("True") | Ok("TRUE")
    )
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // pg_bigm は LIKE + gin_bigm_ops で使う拡張であり、text search configuration ではない。
        // USE_PG_BIGM=true の場合は tsvector を避け、GIN(gin_bigm_ops) index を作成する。
        let search_index_sql = if use_pg_bigm() {
            r#"
CREATE INDEX IF NOT EXISTS idx_tasks_title_bigm ON tasks USING GIN(title gin_bigm_ops);
CREATE INDEX IF NOT EXISTS idx_tasks_description_bigm ON tasks USING GIN(description gin_bigm_ops);
"#
        } else {
            r#"
ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS search_vector tsvector
    GENERATED ALWAYS AS (
        to_tsvector('pg_catalog.simple',
            coalesce(title, '') || ' ' || coalesce(description, ''))
    ) STORED;

CREATE INDEX IF NOT EXISTS idx_tasks_search_vector ON tasks USING GIN(search_vector);
"#
        };

        let sql = format!(
            r#"
{search_index_sql}
CREATE TABLE IF NOT EXISTS project_task_views (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    is_shared BOOLEAN NOT NULL DEFAULT false,
    filters JSONB NOT NULL DEFAULT '{{}}',
    sort JSONB NOT NULL DEFAULT '{{}}',
    view_type VARCHAR NOT NULL DEFAULT 'list',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
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
DROP INDEX IF EXISTS idx_tasks_title_bigm;
DROP INDEX IF EXISTS idx_tasks_description_bigm;
ALTER TABLE tasks DROP COLUMN IF EXISTS search_vector;
"#,
            )
            .await?;
        Ok(())
    }
}

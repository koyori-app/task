use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// コミットとタスクのリンク（docs/features/tasks/9.github-tasks.md §2 / §3 のうちコミット分）。
/// PR 実体（forge_pull_requests）に依存する表は PR 連携で足す。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE forge_commits (
                id            UUID PRIMARY KEY,
                project_id    UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                host          VARCHAR NOT NULL,
                host_url      VARCHAR NOT NULL,
                repo_owner    VARCHAR NOT NULL,
                repo_name     VARCHAR NOT NULL,
                sha           VARCHAR(40) NOT NULL,
                message       TEXT NOT NULL,
                author_handle VARCHAR NOT NULL DEFAULT '',
                author_name   VARCHAR NOT NULL,
                committed_at  TIMESTAMPTZ NOT NULL,
                html_url      VARCHAR NOT NULL,
                created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uq_forge_commits_repo_sha
                    UNIQUE (project_id, host, host_url, repo_owner, repo_name, sha)
            )
        "#,
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE forge_commit_links (
                commit_id  UUID NOT NULL REFERENCES forge_commits(id) ON DELETE CASCADE,
                task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (commit_id, task_id)
            )
        "#,
            )
            .await?;
        // タスク側から引く（タスク削除のカスケードも同じ列を使う）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_forge_commit_links_task ON forge_commit_links(task_id)",
            )
            .await?;
        // 配信 ID はホストのインスタンス内でしか一意でないので host_url まで含める
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE forge_webhook_deliveries (
                id          UUID PRIMARY KEY,
                host        VARCHAR NOT NULL,
                host_url    VARCHAR NOT NULL,
                delivery_id VARCHAR NOT NULL,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uq_forge_webhook_deliveries_delivery
                    UNIQUE (host, host_url, delivery_id)
            )
        "#,
            )
            .await?;
        // 連携由来のアクティビティだけが冪等キーを持つ（既存行と手で行う操作は NULL のまま）
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE task_activities ADD COLUMN dedupe_key VARCHAR")
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE UNIQUE INDEX uq_task_activities_dedupe_key
                 ON task_activities(dedupe_key) WHERE dedupe_key IS NOT NULL",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP INDEX IF EXISTS uq_task_activities_dedupe_key;
                 ALTER TABLE task_activities DROP COLUMN IF EXISTS dedupe_key;
                 DROP TABLE IF EXISTS forge_webhook_deliveries;
                 DROP TABLE IF EXISTS forge_commit_links;
                 DROP TABLE IF EXISTS forge_commits;",
            )
            .await?;
        Ok(())
    }
}

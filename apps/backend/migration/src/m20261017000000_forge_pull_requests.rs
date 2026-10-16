use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// PR とタスクのリンク（docs/features/tasks/9.github-tasks.md §2 / §3 のうち PR 分）。
/// `reviews` の FK 付け替え（§3 の後半）はこのマイグレーションでは行わない。
/// `forge_pull_request_commits` / `forge_branch_links` はコミット集合の同期・ブランチのリンクで足す。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 自然キーに host / host_url / リポジトリまで含めるのは、連携先を差し替えた後の
        // 同番号 PR を別の行にするため（§2）
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE forge_pull_requests (
                id                            UUID PRIMARY KEY,
                project_id                    UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                host                          VARCHAR NOT NULL,
                host_url                      VARCHAR NOT NULL,
                repo_owner                    VARCHAR NOT NULL,
                repo_name                     VARCHAR NOT NULL,
                number                        INT NOT NULL,
                title                         VARCHAR NOT NULL DEFAULT '',
                author_handle                 VARCHAR NOT NULL DEFAULT '',
                state                         VARCHAR NOT NULL DEFAULT 'unknown',
                head_sha                      VARCHAR(40),
                html_url                      VARCHAR NOT NULL DEFAULT '',
                merged_at                     TIMESTAMPTZ,
                host_updated_at               TIMESTAMPTZ,
                merge_effects_processed_at    TIMESTAMPTZ,
                merge_effects_acknowledged_by UUID REFERENCES users(id) ON DELETE SET NULL,
                commits_complete              BOOLEAN NOT NULL DEFAULT false,
                created_at                    TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at                    TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uq_forge_pull_requests_repo_number
                    UNIQUE (project_id, host, host_url, repo_owner, repo_name, number)
            )
        "#,
            )
            .await?;
        // 参照元（source）ごとに 1 行。同じタスクをタイトルと本文の両方が指すと 2 行になる
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE forge_pull_request_links (
                pull_request_id UUID NOT NULL REFERENCES forge_pull_requests(id) ON DELETE CASCADE,
                task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                closes          BOOLEAN NOT NULL DEFAULT false,
                source          VARCHAR NOT NULL,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (pull_request_id, task_id, source)
            )
        "#,
            )
            .await?;
        // タスク側から引く（タスク削除のカスケードも同じ列を使う）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_forge_pr_links_task ON forge_pull_request_links(task_id)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP TABLE IF EXISTS forge_pull_request_links;
                 DROP TABLE IF EXISTS forge_pull_requests;",
            )
            .await?;
        Ok(())
    }
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// PR ↔ コミットの中間表とブランチのリンク（docs/features/tasks/9.github-tasks.md §2 / §3 の残り）。
/// `reviews` の FK 付け替えはこのマイグレーションでも行わない。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 同じコミットが複数の PR に属する（stacked PR 等）ので多対多で持つ（§2）
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE forge_pull_request_commits (
                pull_request_id UUID NOT NULL REFERENCES forge_pull_requests(id) ON DELETE CASCADE,
                commit_id       UUID NOT NULL REFERENCES forge_commits(id) ON DELETE CASCADE,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (pull_request_id, commit_id)
            )
        "#,
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE forge_branch_links (
                id         UUID PRIMARY KEY,
                project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                host       VARCHAR NOT NULL,
                host_url   VARCHAR NOT NULL,
                repo_owner VARCHAR NOT NULL,
                repo_name  VARCHAR NOT NULL,
                branch     VARCHAR NOT NULL,
                html_url   VARCHAR NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uq_forge_branch_links_task_branch
                    UNIQUE (task_id, host, host_url, repo_owner, repo_name, branch)
            )
        "#,
            )
            .await?;
        // コミット削除のカスケードと、ブランチ削除イベントで引く列
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_forge_pr_commits_commit ON forge_pull_request_commits(commit_id);
                 CREATE INDEX idx_forge_branch_links_task ON forge_branch_links(task_id);",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP TABLE IF EXISTS forge_branch_links;
                 DROP TABLE IF EXISTS forge_pull_request_commits;",
            )
            .await?;
        Ok(())
    }
}

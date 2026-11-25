use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// PR の役割（実装 / 言及）の手の上書きと、PR の head のブランチ名
/// （docs/dev/features/tasks/github-tasks/index.md の「データ」）。
///
/// 自動の判定は保存しない（応答のたびに題とリンクの行から計算する）ので、既存の行は書き換えない。
/// `head_branch` は次の PR イベントで埋まる。
///
/// 起動時の SeaORM `sync()` が entity から表と列を先に作ることがあるので、すべて `IF NOT EXISTS` にする。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE forge_pull_requests ADD COLUMN IF NOT EXISTS head_branch VARCHAR",
            )
            .await?;
        // PR × タスクにつき 1 行。行があるときだけ自動の判定に勝つ
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE IF NOT EXISTS forge_pull_request_roles (
                pull_request_id UUID NOT NULL REFERENCES forge_pull_requests(id) ON DELETE CASCADE,
                task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                role            VARCHAR NOT NULL,
                set_by          UUID REFERENCES users(id) ON DELETE SET NULL,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (pull_request_id, task_id)
            )
        "#,
            )
            .await?;
        // タスク側から引く（タスク削除のカスケードも同じ列を使う）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_forge_pr_roles_task ON forge_pull_request_roles(task_id)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP TABLE IF EXISTS forge_pull_request_roles;
                 ALTER TABLE forge_pull_requests DROP COLUMN IF EXISTS head_branch;",
            )
            .await?;
        Ok(())
    }
}

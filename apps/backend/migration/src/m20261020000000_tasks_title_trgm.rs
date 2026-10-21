use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

/// タスク検索の部分一致・表記ゆれ用に pg_trgm と title の trigram index を足す
/// （apps/backend/docs/task-similarity-search.md §4）。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("CREATE EXTENSION IF NOT EXISTS pg_trgm")
            .await?;

        // lc_ctype が C だと CJK の trigram が作られず、日本語の検索が黙って効かなくなる
        let trigrams: Option<i32> = db
            .query_one_raw(Statement::from_string(
                manager.get_database_backend(),
                "SELECT array_length(show_trgm('日本語テスト'), 1) AS n",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("show_trgm の結果が返らない".into()))?
            .try_get("", "n")?;
        if trigrams.unwrap_or(0) == 0 {
            return Err(DbErr::Custom(
                "pg_trgm が日本語の trigram を作らない（DB の lc_ctype が C の可能性）。\
                 UTF-8 のロケール（例 en_US.utf8）で作った DB を使う"
                    .into(),
            ));
        }

        db.execute_unprepared(
            "CREATE INDEX idx_tasks_title_trgm ON tasks USING GIN (title gin_trgm_ops)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 拡張は既存を流用した可能性があり、他の依存で DROP できないことがあるので残す
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_tasks_title_trgm")
            .await?;
        Ok(())
    }
}

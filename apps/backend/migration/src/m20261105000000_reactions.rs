use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// コメントとタスク本体の絵文字リアクション（docs/features/tasks/4.collaboration.md §1.3）。
///
/// 対象は `target_type` + `target_id` で指す多態の参照なので、`target_id` に FK は張れない。
/// 消し込みは `task_id` の CASCADE に任せる（コメント向けでもそのコメントの task_id を入れる）。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE reactions (
                id          UUID PRIMARY KEY,
                task_id     UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                target_type TEXT NOT NULL,
                target_id   UUID NOT NULL,
                user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                emoji       TEXT NOT NULL,
                created_at  TIMESTAMPTZ NOT NULL,
                CONSTRAINT reactions_target_type_target_id_user_id_emoji_key
                    UNIQUE (target_type, target_id, user_id, emoji)
            )
        "#,
            )
            .await?;
        // 1 対象の一覧と、コメント一覧がタスク単位でまとめて引く列
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_reactions_target ON reactions(target_type, target_id);
                 CREATE INDEX idx_reactions_task ON reactions(task_id);",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS reactions;")
            .await?;
        Ok(())
    }
}

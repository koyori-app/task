use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // 指摘の内容を最後に直した時刻（TASK-577）。直していなければ NULL
        db.execute_unprepared(
            "ALTER TABLE review_findings ADD COLUMN IF NOT EXISTS edited_at TIMESTAMPTZ",
        )
        .await?;
        // 内容の直しの履歴。状態の遷移（review_finding_transitions）とは別の表に置く。
        // 遷移の表は from_state / to_state を持ち、from_state が NULL の行を起票と読むので、
        // 状態の変わらない直しを混ぜるとその読み方が崩れる。
        // 1 回の直しで変わった欄ごとに 1 行を残し、同じ直しの行は edit_id で束ねる。
        // 値は文字列で持つ（line は 10 進の字、NULL は「無い」）。
        // actor_id は著者性の記録なので、遷移と同じく NO ACTION（利用者削除は墓標方式）
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS review_finding_edits (
                id           UUID PRIMARY KEY,
                finding_id   UUID NOT NULL REFERENCES review_findings(id) ON DELETE CASCADE,
                edit_id      UUID NOT NULL,
                actor_id     UUID NOT NULL REFERENCES users(id) ON DELETE NO ACTION,
                field        VARCHAR NOT NULL
                    CHECK (field IN ('title', 'body', 'file', 'line')),
                before_value TEXT,
                after_value  TEXT,
                created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
            )
        "#,
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_review_finding_edits_finding ON review_finding_edits(finding_id, created_at)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS review_finding_edits")
            .await?;
        db.execute_unprepared("ALTER TABLE review_findings DROP COLUMN IF EXISTS edited_at")
            .await?;
        Ok(())
    }
}

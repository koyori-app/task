use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 引き継ぎの写しだけが持つ列。UNIQUE は足さない（部分 UNIQUE は起動時の sync と食い違う）。
        // 重複はプロジェクト行のロックの下で確かめる
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE reviews ADD COLUMN IF NOT EXISTS carried_from_review_id UUID REFERENCES reviews(id) ON DELETE SET NULL",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE reviews ADD COLUMN IF NOT EXISTS carried_by UUID REFERENCES users(id) ON DELETE NO ACTION",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE review_findings ADD COLUMN IF NOT EXISTS carried_from_finding_id UUID REFERENCES review_findings(id) ON DELETE SET NULL",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_review_findings_carried_from ON review_findings(carried_from_finding_id)",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE review_findings ADD COLUMN IF NOT EXISTS carried_root_finding_id UUID",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE review_findings DROP COLUMN IF EXISTS carried_root_finding_id",
        )
        .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_review_findings_carried_from")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE review_findings DROP COLUMN IF EXISTS carried_from_finding_id",
        )
        .await?;
        db.execute_unprepared("ALTER TABLE reviews DROP COLUMN IF EXISTS carried_by")
            .await?;
        db.execute_unprepared("ALTER TABLE reviews DROP COLUMN IF EXISTS carried_from_review_id")
            .await?;
        Ok(())
    }
}

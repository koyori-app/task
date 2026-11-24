use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // projects.icon_emoji と同じ形。未設定は NULL で、画面は色の丸を出す。
        // 名前の先頭に絵文字を入れている既存のラベルは書き換えない
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE labels ADD COLUMN IF NOT EXISTS icon_emoji VARCHAR")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE labels DROP COLUMN IF EXISTS icon_emoji")
            .await?;
        Ok(())
    }
}

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // テナント（会社）の中での部署・役職・チーム。任意の自由記述
        let conn = manager.get_connection();
        for column in ["department", "position", "team"] {
            conn.execute_unprepared(&format!(
                "ALTER TABLE tenant_members ADD COLUMN IF NOT EXISTS {column} TEXT"
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        for column in ["department", "position", "team"] {
            conn.execute_unprepared(&format!(
                "ALTER TABLE tenant_members DROP COLUMN IF EXISTS {column}"
            ))
            .await?;
        }
        Ok(())
    }
}

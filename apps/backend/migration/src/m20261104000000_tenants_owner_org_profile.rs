use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // テナントオーナーの部署・役職・チーム。オーナーは `tenant_members` に行を持たず、
        // 行を足すと通知の宛先などが変わるので、テナントの行に持つ
        let conn = manager.get_connection();
        for column in ["owner_department", "owner_position", "owner_team"] {
            conn.execute_unprepared(&format!(
                "ALTER TABLE tenants ADD COLUMN IF NOT EXISTS {column} TEXT"
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        for column in ["owner_department", "owner_position", "owner_team"] {
            conn.execute_unprepared(&format!(
                "ALTER TABLE tenants DROP COLUMN IF EXISTS {column}"
            ))
            .await?;
        }
        Ok(())
    }
}

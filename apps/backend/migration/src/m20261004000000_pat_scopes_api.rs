use sea_orm_migration::prelude::*;

/// SQL の本体は `apps/backend/sql/` に置いて統合テストと共有する
/// （`m20260904000000_drive_project_id_backfill` と同じ理由）。
const MIGRATE_SQL: &str = include_str!("../../sql/migrate_pat_scopes_api.sql");

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(MIGRATE_SQL)
            .await?;
        Ok(())
    }

    /// 巻き戻さない。
    ///
    /// `admin:project` を展開した後の write:* は、もともと列挙で持っていた値と見分けが
    /// つかない。`api` を `admin:tenant` に戻すことはできるが、片方だけ戻しても旧版の
    /// コードは `read:tenant` などの新しい値を読めないので、戻す意味が無い。
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

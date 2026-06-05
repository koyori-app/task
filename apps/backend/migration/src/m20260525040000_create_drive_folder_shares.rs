
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS drive_folder_shares (
                id UUID PRIMARY KEY,
                folder_id UUID NOT NULL REFERENCES drive_folders(id) ON DELETE CASCADE,
                shared_with_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
                share_token VARCHAR UNIQUE,
                permission VARCHAR(16) NOT NULL,
                created_by UUID NOT NULL REFERENCES users(id) ON DELETE NO ACTION,
                expires_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT drive_folder_shares_xor_check CHECK (
                    (shared_with_user_id IS NOT NULL)::int + (share_token IS NOT NULL)::int = 1
                )
            )
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = "DROP TABLE IF EXISTS drive_folder_shares";
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

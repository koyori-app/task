
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql1 = r#"
            ALTER TABLE personal_tokens
              ADD COLUMN IF NOT EXISTS tenant_id UUID NULL REFERENCES tenants(id) ON DELETE CASCADE,
              ADD COLUMN IF NOT EXISTS allowed_project_ids JSONB NULL
        "#;
                manager.get_connection().execute_unprepared(sql1).await.map(|_| ())?;

        // 既存行はテナントにバインドできないため削除する
        let sql2 = r#"DELETE FROM personal_tokens WHERE tenant_id IS NULL"#;
                manager.get_connection().execute_unprepared(sql2).await.map(|_| ())?;

        let sql3 = r#"ALTER TABLE personal_tokens ALTER COLUMN tenant_id SET NOT NULL"#;
                manager.get_connection().execute_unprepared(sql3).await.map(|_| ())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let sql = r#"
            ALTER TABLE personal_tokens
              DROP COLUMN IF EXISTS allowed_project_ids,
              DROP COLUMN IF EXISTS tenant_id
        "#;
                manager.get_connection().execute_unprepared(sql).await.map(|_| ())
    }
}

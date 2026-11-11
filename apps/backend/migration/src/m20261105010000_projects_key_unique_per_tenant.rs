use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// プロジェクトキーの一意をテナント単位に直す。
///
/// entity の `tenant_id` の `unique_key` が個人プロジェクト用の複合に使われていたので、
/// schema sync が `key` だけの UNIQUE を作り、別テナントで同じキーを使えなかった。
/// entity は (tenant_id, key) に直し、(tenant_id, personal_owner_id) はアプリ側のロックで守る。
/// entity に無い UNIQUE が残ると sync が DROP CONSTRAINT で落ちるので、sync より先にここで消す。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP INDEX IF EXISTS "idx-projects-projects_key_tenant_unique";
                DROP INDEX IF EXISTS "idx-projects-idx_projects_personal_owner";
                DROP INDEX IF EXISTS idx_projects_personal_owner;
                CREATE UNIQUE INDEX IF NOT EXISTS projects_key_tenant_unique
                    ON projects(tenant_id, key);
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // key だけの UNIQUE には戻さない（別テナントで同じキーを使っていると作れない）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_personal_owner \
                 ON projects(tenant_id, personal_owner_id) WHERE is_personal = true",
            )
            .await?;
        Ok(())
    }
}

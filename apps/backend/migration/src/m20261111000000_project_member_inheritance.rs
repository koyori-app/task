use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // プロジェクトがテナントのメンバーを継承するか（TASK-501）。
        // 既存のプロジェクトはすべて継承する形にする。今までの絞り（メンバーを一人でも載せると
        // 載せた人だけに絞る）は役を付けるために生まれた物で、意図した物ではないと見るため。
        // project_members の行と役は触らない
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE projects
                ADD COLUMN inherit_tenant_members boolean NOT NULL DEFAULT true;
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 旧版は、メンバーを載せたプロジェクトを載せた人だけに絞る。戻すと、継承していた
        // プロジェクトのうちメンバーを載せた物は、載せた人だけに絞られる
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE projects DROP COLUMN inherit_tenant_members;
        "#,
            )
            .await?;
        Ok(())
    }
}

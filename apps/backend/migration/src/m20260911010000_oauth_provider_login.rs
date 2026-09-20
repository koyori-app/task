use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // プロバイダー上のログイン名（小文字）。コミット作者を Task ユーザーに解決するのに使う。
        // 一意性は UNIQUE 制約ではなく保存時の付け替えで保つ。entity は 1 列に unique_key を 1 つしか
        // 持てず、provider は既存の複合 UNIQUE に入っているので (provider, provider_login) を表せない。
        // 表せない UNIQUE 制約は起動時の schema sync が黙って DROP する
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE oauth_connections ADD COLUMN provider_login VARCHAR")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE oauth_connections DROP COLUMN IF EXISTS provider_login")
            .await?;
        Ok(())
    }
}

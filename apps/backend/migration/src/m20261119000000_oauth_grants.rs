use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// MCP の OAuth の許可を `personal_tokens` の行に載せる（`oauth_client_id` が NULL でない行）。
    /// アクセストークンとリフレッシュトークンは `oauth_tokens` に置き、使うたびに親の許可を引く。
    /// 新しいバックエンドをマイグレーションより先に起動すると SeaORM の `sync()` が列と表を
    /// 作るので、`IF NOT EXISTS` で受ける。`token_hash` の一意は列の UNIQUE で持つ
    /// （部分 UNIQUE は起動時の schema sync が落とす）。
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared(
            "ALTER TABLE personal_tokens ADD COLUMN IF NOT EXISTS oauth_client_id VARCHAR",
        )
        .await?;
        conn.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS oauth_tokens (
                id          UUID PRIMARY KEY,
                grant_id    UUID NOT NULL REFERENCES personal_tokens(id) ON DELETE CASCADE,
                kind        VARCHAR NOT NULL,
                token_hash  VARCHAR NOT NULL UNIQUE,
                expires_at  TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
            )
        "#,
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_oauth_tokens_grant_id ON oauth_tokens(grant_id)",
        )
        .await?;
        Ok(())
    }

    /// 旧版は許可の行を PAT として一覧に出してしまうので、行を残さない。
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared("DROP TABLE IF EXISTS oauth_tokens")
            .await?;
        conn.execute_unprepared("DELETE FROM personal_tokens WHERE oauth_client_id IS NOT NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE personal_tokens DROP COLUMN oauth_client_id")
            .await?;
        Ok(())
    }
}

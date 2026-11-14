use sea_orm_migration::prelude::*;

/// 既存トークンの束縛を中間テーブルへ移す SQL。統合テストと共有するため
/// `apps/backend/sql/` に置く（`m20261004000000_pat_scopes_api` と同じ置き方）。
const BACKFILL_SQL: &str = include_str!("../../sql/backfill_personal_token_tenants.sql");

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// PAT を複数のテナントに結び付けられるよう、束縛を `personal_tokens.tenant_id` 列から
    /// 中間テーブルへ移し、列を消す。既存トークンは 1 行ずつ移るので、移行後も同じ 1 テナントで
    /// だけ通る。一意性は複合主キーで持ち、UNIQUE インデックスは足さない。
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared(
            "CREATE TABLE personal_token_tenants (
                token_id  UUID NOT NULL REFERENCES personal_tokens(id) ON DELETE CASCADE,
                tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                PRIMARY KEY (token_id, tenant_id)
            )",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX idx_personal_token_tenants_tenant ON personal_token_tenants(tenant_id)",
        )
        .await?;
        conn.execute_unprepared(BACKFILL_SQL).await?;
        conn.execute_unprepared("ALTER TABLE personal_tokens DROP COLUMN tenant_id")
            .await?;
        Ok(())
    }

    /// 旧版は 1 トークン 1 テナントなので、束縛が 2 件以上のトークンは表せない。
    /// 最小の `tenant_id` で列を埋めて取り消し、旧版で表せない範囲を黙って残さない。
    /// 束縛の無いトークン（テナントの削除や一括失効のあとに行だけ残ったもの）は消す。
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared("ALTER TABLE personal_tokens ADD COLUMN tenant_id UUID")
            .await?;
        conn.execute_unprepared(
            "UPDATE personal_tokens t
             SET tenant_id = b.min_tenant_id,
                 revoked = t.revoked OR b.bindings > 1
             FROM (
                 SELECT token_id, MIN(tenant_id::text)::uuid AS min_tenant_id, COUNT(*) AS bindings
                 FROM personal_token_tenants
                 GROUP BY token_id
             ) b
             WHERE b.token_id = t.id",
        )
        .await?;
        conn.execute_unprepared("DELETE FROM personal_tokens WHERE tenant_id IS NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE personal_tokens ALTER COLUMN tenant_id SET NOT NULL")
            .await?;
        conn.execute_unprepared(
            "ALTER TABLE personal_tokens
             ADD CONSTRAINT personal_tokens_tenant_id_fkey
             FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE",
        )
        .await?;
        conn.execute_unprepared("DROP TABLE personal_token_tenants")
            .await?;
        Ok(())
    }
}

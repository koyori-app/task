use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // テナントへのメール招待（apps/backend/docs/tenant-project-authz.md の「招待」）。
        // 承諾・取り消しで行を消すので、行があること = 保留中。同じ宛先への再招待は
        // (tenant_id, email) の UNIQUE で同じ行を作り直す。
        // token_hash はメール送信ジョブが発行するまで NULL（平文のトークンはどこにも残さない）。
        // 列の UNIQUE で持つ（部分 UNIQUE は起動時の schema sync が落とす）
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE tenant_invitations (
                id          UUID PRIMARY KEY,
                tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                email       VARCHAR NOT NULL,
                role        VARCHAR(255) NOT NULL,
                token_hash  VARCHAR UNIQUE,
                invited_by  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                expires_at  TIMESTAMPTZ NOT NULL,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT tenant_invitations_tenant_id_email_key UNIQUE (tenant_id, email)
            )
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS tenant_invitations")
            .await?;
        Ok(())
    }
}

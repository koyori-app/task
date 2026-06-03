use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS audit_logs (
                    id UUID PRIMARY KEY,
                    actor_id UUID REFERENCES users(id) ON DELETE SET NULL,
                    actor_type VARCHAR NOT NULL,
                    action VARCHAR NOT NULL,
                    resource_type VARCHAR NOT NULL,
                    resource_id VARCHAR NOT NULL,
                    tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
                    metadata JSONB,
                    ip_address VARCHAR(45),
                    user_agent VARCHAR,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                );
                CREATE INDEX IF NOT EXISTS idx_audit_logs_actor_id ON audit_logs(actor_id);
                CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
                CREATE INDEX IF NOT EXISTS idx_audit_logs_resource ON audit_logs(resource_type, resource_id);
                CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_id ON audit_logs(tenant_id);
                CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at DESC);
                REVOKE UPDATE, DELETE ON audit_logs FROM app_role;",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS audit_logs;")
            .await?;
        Ok(())
    }
}

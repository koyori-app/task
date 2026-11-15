-- 起動時の SeaORM sync() が先にテーブルを作っていても通るよう、IF NOT EXISTS にする。
CREATE TABLE IF NOT EXISTS personal_token_tenants (
    token_id  UUID NOT NULL REFERENCES personal_tokens(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    PRIMARY KEY (token_id, tenant_id)
);
CREATE INDEX IF NOT EXISTS idx_personal_token_tenants_tenant ON personal_token_tenants(tenant_id);

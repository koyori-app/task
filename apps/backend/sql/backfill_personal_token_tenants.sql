INSERT INTO personal_token_tenants (token_id, tenant_id)
SELECT id, tenant_id FROM personal_tokens
ON CONFLICT DO NOTHING;

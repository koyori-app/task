#!/usr/bin/env bash
# Apply migration-crate schema, prepare for SeaORM sync, then start backend.
# Playwright starts webServers before globalSetup, so migration must run here.
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=resolve-database-url.sh
source "$SCRIPT_DIR/resolve-database-url.sh"
export_e2e_database_urls

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MIGRATION_DIR="$ROOT/apps/backend/migration"

if [ -n "${MIGRATION_BIN:-}" ]; then
  "$MIGRATION_BIN" fresh
else
  cargo run --manifest-path "$MIGRATION_DIR/Cargo.toml" -- fresh
fi

# SeaORM sync() treats some migration UNIQUE INDEXes as constraints and fails on DROP.
# Same indexes are dropped in apps/backend/tests/common/mod.rs before sync().
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DROP INDEX IF EXISTS projects_key_tenant_unique;
DROP INDEX IF EXISTS labels_project_name_unique;
DROP INDEX IF EXISTS idx_sprints_active_per_project;
DROP INDEX IF EXISTS idx_projects_personal_owner;
SQL

if [ -n "${BACKEND_BIN:-}" ]; then
  exec "$BACKEND_BIN"
fi

cd "$ROOT/apps/backend"
exec cargo run --bin backend

#!/usr/bin/env bash
# Apply migration-crate schema, then start the backend for Playwright webServer.
# Playwright starts webServers before globalSetup, so migration must run here.
set -euo pipefail

DB_URL="${E2E_DATABASE_URL:-postgresql://test:test@localhost:5432/task_e2e}"
export DATABASE_URL="$DB_URL"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MIGRATION_DIR="$ROOT/apps/backend/migration"

if [[ -n "${MIGRATION_BIN:-}" ]]; then
  "$MIGRATION_BIN" fresh
else
  cargo run --manifest-path "$MIGRATION_DIR/Cargo.toml" -- fresh
fi

if [[ -n "${BACKEND_BIN:-}" ]]; then
  exec "$BACKEND_BIN"
fi

cd "$ROOT/apps/backend"
exec cargo run --bin backend

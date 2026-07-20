#!/usr/bin/env bash
# マイグレーション適用 → SeaORM の schema sync が構造上ドロップできない UNIQUE INDEX を先に落とす。
# e2e/scripts/start-backend.sh と違い `up`（冪等・データ保持）を使う。
# `fresh` は全テーブル DROP のため、compose の depends_on 経由で再実行されるたびに
# 開発データが全消去され、apalis の適用記録（public._sqlx_migrations）も飛んで
# backend が 42P06（schema "apalis" already exists）で起動不能になる（2026-07-16 に2回実発生）。
# まっさらにしたい時だけ手動で: docker compose run --rm migration /app/migration fresh
set -euo pipefail

/app/migration up

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DROP INDEX IF EXISTS projects_key_tenant_unique;
DROP INDEX IF EXISTS labels_project_name_unique;
DROP INDEX IF EXISTS idx_sprints_active_per_project;
DROP INDEX IF EXISTS idx_projects_personal_owner;
SQL

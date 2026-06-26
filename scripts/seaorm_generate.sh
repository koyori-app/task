#!/usr/bin/env bash
# Regenerate SeaORM entities from DB schema and apply OpenAPI postprocess.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND="$ROOT/apps/backend"

if [[ -f "$BACKEND/.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "$BACKEND/.env"
  set +a
fi

: "${database_url:?database_url must be set in apps/backend/.env}"

TABLES="${1:-}"
if [[ -z "$TABLES" ]]; then
  echo "usage: $0 <comma-separated-table-names>" >&2
  exit 1
fi

mkdir -p "$BACKEND/src/entities/_generated"

sea-orm-cli generate entity \
  --database-url "$database_url" \
  --output-dir "$BACKEND/src/entities/_generated" \
  --tables "$TABLES" \
  --entity-format dense \
  --with-serde serialize \
  --date-time-crate chrono \
  --with-prelude none \
  --impl-active-model-behavior \
  --model-extra-derives 'utoipa::ToSchema'

bash "$ROOT/scripts/seaorm_postprocess.sh"

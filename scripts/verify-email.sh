#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -z "${E2E_DATABASE_URL:-}" && -z "${DATABASE_URL:-}" ]]; then
  backend_env="apps/backend/.env"
  if [[ -f "$backend_env" ]]; then
    # Best-effort parser: expects KEY=value, uppercase keys, and values without '='.
    db_url=$(awk '
      /^[[:space:]]*(#|$)/ { next }
      {
        line = $0
        sub(/^[[:space:]]*export[[:space:]]+/, "", line)
        if (line ~ /^[[:space:]]*database_url[[:space:]]*=/) {
          sub(/^[^=]*=/, "", line)
          gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
          first = substr(line, 1, 1)
          last = substr(line, length(line), 1)
          if ((first == "\"" && last == "\"") || (first == sprintf("%c", 39) && last == sprintf("%c", 39))) {
            line = substr(line, 2, length(line) - 2)
          }
          print line
          exit
        }
      }
    ' "$backend_env")
    if [[ -n "$db_url" ]]; then
      export DATABASE_URL="$db_url"
    fi
  fi
fi

exec bun e2e/scripts/verify-email.ts "$@"

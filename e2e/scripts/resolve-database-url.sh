#!/usr/bin/env bash
# Shared e2e DB URL resolution (mirrors e2e/env.ts precedence).
set -eu

E2E_DEFAULT_URL='postgresql://test:test@localhost:5432/task_e2e'

_read_env_key() {
  local file="$1" key="$2"
  [[ -f "$file" ]] || return 0
  awk -v key="$key" '
    /^[[:space:]]*(#|$)/ { next }
    {
      line = $0
      sub(/^[[:space:]]*export[[:space:]]+/, "", line)
      split(line, parts, "=")
      k = parts[1]
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      if (k != key) next
      v = substr(line, index(line, "=") + 1)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      first = substr(v, 1, 1)
      last = substr(v, length(v), 1)
      if ((first == "\"" && last == "\"") || (first == "\047" && last == "\047")) {
        v = substr(v, 2, length(v) - 2)
      }
      print v
      exit
    }
  ' "$file"
}

resolve_e2e_database_url() {
  local e2e_dir root backend_env file key url

  e2e_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  root="$(cd "$e2e_dir/.." && pwd)"
  backend_env="$root/apps/backend/.env"

  if [[ -n "${E2E_DATABASE_URL:-}" ]]; then
    printf '%s' "$E2E_DATABASE_URL"
    return
  fi
  if [[ -n "${DATABASE_URL:-}" ]]; then
    printf '%s' "$DATABASE_URL"
    return
  fi

  for file in "$e2e_dir/.env" "$root/.env" "$backend_env"; do
    for key in E2E_DATABASE_URL DATABASE_URL database_url; do
      url="$(_read_env_key "$file" "$key")"
      if [[ -n "$url" ]]; then
        printf '%s' "$url"
        return
      fi
    done
  done

  printf '%s' "$E2E_DEFAULT_URL"
}

export_e2e_database_urls() {
  local url
  url="$(resolve_e2e_database_url)"
  export E2E_DATABASE_URL="$url"
  export DATABASE_URL="$url"
}

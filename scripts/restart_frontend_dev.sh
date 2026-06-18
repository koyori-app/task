#!/usr/bin/env bash
# restart_frontend_dev.sh — frontend dev server only (no backend)
# Usage: bash scripts/restart_frontend_dev.sh [--foreground]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRONTEND_DIR="$ROOT/apps/frontend"
LOG_FILE="${FRONTEND_DEV_LOG:-/tmp/koyori-frontend-dev.log}"
DEV_PORT="${FRONTEND_DEV_PORT:-3000}"
FOREGROUND=0

if [[ "${1:-}" == "--foreground" ]]; then
  FOREGROUND=1
fi

pids_on_port() {
  local port="$1"
  ss -tlnp 2>/dev/null \
    | grep -E "[:.]${port}[[:space:]]" \
    | sed -n "s/.*pid=\\([0-9]*\\).*/\\1/p" \
    | sort -u
}

kill_pids() {
  local pids="$1"
  [[ -z "${pids}" ]] && return 0
  kill ${pids} 2>/dev/null || true
  sleep 1
  kill -9 ${pids} 2>/dev/null || true
}

kill_frontend_dev() {
  # Port listeners (vike dev). Never touch backend :3400.
  kill_pids "$(pids_on_port "${DEV_PORT}")"

  # Orphan pnpm/vike dev processes for this repo frontend
  while IFS= read -r line; do
    [[ -z "${line}" ]] && continue
    local pid="${line%% *}"
    local cmd="${line#* }"
    if [[ "${cmd}" == *"${FRONTEND_DIR}"* ]] \
      || [[ "${cmd}" == *"vike dev"* && "${cmd}" != *"backend"* ]]; then
      kill_pids "${pid}"
    fi
  done < <(pgrep -af "pnpm run dev|vike dev" 2>/dev/null || true)

  sleep 1
}

wait_for_port() {
  local i
  for i in $(seq 1 30); do
    if [[ -n "$(pids_on_port "${DEV_PORT}")" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "Error: frontend dev did not listen on port ${DEV_PORT} within 30s" >&2
  echo "See log: ${LOG_FILE}" >&2
  return 1
}

kill_frontend_dev

cd "${FRONTEND_DIR}"

if [[ "${FOREGROUND}" -eq 1 ]]; then
  exec pnpm run dev
fi

nohup pnpm run dev >"${LOG_FILE}" 2>&1 &
echo "frontend dev starting (port ${DEV_PORT}). log: ${LOG_FILE}"
wait_for_port
echo "frontend dev ready on port ${DEV_PORT} (pid $(pids_on_port "${DEV_PORT}" | tr "\n" " "))"

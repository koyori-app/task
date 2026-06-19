#!/usr/bin/env bash
# restart_backend_dev.sh — backend dev server only (no frontend)
# Usage: bash scripts/restart_backend_dev.sh [--foreground]
export SQLX_OFFLINE=true
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_DIR="$ROOT/apps/backend"
LOG_FILE="${BACKEND_DEV_LOG:-/tmp/koyori-backend-dev.log}"
DEV_PORT="${BACKEND_DEV_PORT:-3400}"
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

pid_cwd_under_backend() {
  local pid="$1"
  [[ -z "${pid}" || ! "${pid}" =~ ^[0-9]+$ ]] && return 1
  [[ "${pid}" -eq "$$" || "${pid}" -eq "${BASHPID}" ]] && return 1
  local cwd
  cwd="$(readlink -f "/proc/${pid}/cwd" 2>/dev/null)" || return 1
  [[ "${cwd}" == "${BACKEND_DIR}"* ]]
}

kill_backend_dev() {
  # Port listeners (:3400). Never touch frontend dev ports.
  local pid cwd
  for pid in $(pids_on_port "${DEV_PORT}"); do
    if pid_cwd_under_backend "${pid}"; then
      kill_pids "${pid}"
    else
      cwd="$(readlink -f "/proc/${pid}/cwd" 2>/dev/null || echo "?")"
      echo "Error: port ${DEV_PORT} is occupied by non-project process (pid ${pid}, cwd ${cwd}). Aborting." >&2
      exit 1
    fi
  done

  # Orphan cargo/backend processes whose cwd is under BACKEND_DIR
  local pid
  for pid in $(pgrep -f "cargo run --bin backend|target/debug/backend" 2>/dev/null || true); do
    if pid_cwd_under_backend "${pid}"; then
      kill_pids "${pid}"
    fi
  done

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
  echo "Error: backend dev did not listen on port ${DEV_PORT} within 30s" >&2
  echo "See log: ${LOG_FILE}" >&2
  return 1
}

cd "${BACKEND_DIR}"

if [[ -f ./.env ]]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

echo "Building backend (SQLX_OFFLINE=true)..."
if ! cargo build --bin backend; then
  echo "Error: cargo build failed; existing dev server left running on port ${DEV_PORT}" >&2
  exit 1
fi

kill_backend_dev

if [[ "${FOREGROUND}" -eq 1 ]]; then
  exec ./target/debug/backend
fi

nohup ./target/debug/backend >"${LOG_FILE}" 2>&1 &
echo "backend dev starting (port ${DEV_PORT}). log: ${LOG_FILE}"
wait_for_port
echo "backend dev ready on port ${DEV_PORT} (pid $(pids_on_port "${DEV_PORT}" | tr "\n" " "))"

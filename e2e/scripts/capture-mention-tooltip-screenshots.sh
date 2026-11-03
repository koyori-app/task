#!/usr/bin/env bash
# Coder 上で origin/main（修正前）と fix/mention-tooltip-layer（修正後）のスクショを撮る。
# 用法（Coder SSH 先）:
#   timeout 20m bash e2e/scripts/capture-mention-tooltip-screenshots.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
E2E="$ROOT/e2e"
ART="$ROOT/memo/TASK-412-screenshots"
TAG=ashigaru7-uicheck300
TMUX_SESSION="uicheck-${TAG}"
FRONTEND_PORT=5174

mkdir -p "$ART/before" "$ART/after"

export PATH="${HOME}/.local/share/pnpm:${PATH:-/usr/bin:/bin}"
command -v corepack >/dev/null && export PATH="$(dirname "$(command -v corepack)"):$PATH"

stop_frontend() {
  tmux send-keys -t "${TMUX_SESSION}:frontend" C-c 2>/dev/null || true
  sleep 2
}

start_frontend() {
  tmux send-keys -t "${TMUX_SESSION}:frontend" \
    "cd ${ROOT}/apps/frontend && API_BASE=http://127.0.0.1:13400 pnpm exec vike dev --host 127.0.0.1 --port ${FRONTEND_PORT}" C-m
  for _ in $(seq 1 60); do
    if curl -sf "http://127.0.0.1:${FRONTEND_PORT}/" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  echo "frontend did not become ready" >&2
  return 1
}

run_playwright() {
  local label=$1
  local outdir=$2
  cd "$E2E"
  MENTION_SCREENSHOT_DIR="$outdir" \
  MENTION_SHOT_LABEL="$label" \
  BASE_URL="http://127.0.0.1:${FRONTEND_PORT}" \
  API_URL="http://127.0.0.1:13400" \
  pnpm exec playwright test mention-tooltip.authenticated.spec.ts \
    --project=chromium-authenticated \
    --project=webkit-authenticated
}

if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
  echo "start uicheck stack first (uicheck_coder_stack.sh start)" >&2
  exit 2
fi

cd "$ROOT"
git fetch origin

echo "=== BEFORE (origin/main) ==="
git checkout -f origin/main
stop_frontend
start_frontend
run_playwright before-main "$ART/before" || true

echo "=== AFTER (fix/mention-tooltip-layer) ==="
git checkout -f fix/mention-tooltip-layer
git pull --ff-only origin fix/mention-tooltip-layer
stop_frontend
start_frontend
run_playwright after-fix "$ART/after"

stop_frontend
echo "screenshots under $ART"

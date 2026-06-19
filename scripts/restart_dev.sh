#!/usr/bin/env bash
# restart_dev.sh — restart backend then frontend dev servers (non-foreground)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

bash "${SCRIPT_DIR}/restart_backend_dev.sh"
bash "${SCRIPT_DIR}/restart_frontend_dev.sh"

#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=e2e/scripts/resolve-database-url.sh
source e2e/scripts/resolve-database-url.sh
export_e2e_database_urls

exec bun e2e/scripts/verify-email.ts "$@"

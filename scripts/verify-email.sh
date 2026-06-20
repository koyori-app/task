#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
exec bun e2e/scripts/verify-email.ts "$@"

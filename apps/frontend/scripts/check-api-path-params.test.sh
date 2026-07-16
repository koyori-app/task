#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
checker="$script_dir/check-api-path-params.sh"
safe_fixture="$script_dir/fixtures/safe/src"
anti_pattern_fixture="$script_dir/fixtures/anti-pattern/src"

bash "$checker" "$safe_fixture"

if bash "$checker" "$anti_pattern_fixture"; then
  echo 'expected the anti-pattern fixture to fail, but it passed' >&2
  exit 1
fi

echo 'API path-param gate self-test passed (anti-pattern red, resolved/mocks green)'

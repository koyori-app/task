#!/usr/bin/env bash
# check_rebase.sh -- リベース後の退行チェック
# 使用法: bash scripts/check_rebase.sh <branch>
set -euo pipefail

BRANCH="${1:-}"
if [[ -z "$BRANCH" ]]; then
  echo "Usage: $0 <branch>" >&2
  exit 1
fi

ERRORS=0

check_lines_present() {
  local file="$1"
  local branch="$2"

  while IFS= read -r line; do
    [[ -z "${line// }" ]] && continue
    if ! git show "origin/$branch:$file" 2>/dev/null | grep -qF "$line"; then
      echo "MISSING in $branch/$file: $line"
      ERRORS=$((ERRORS + 1))
    fi
  done < <(git show "origin/main:$file" 2>/dev/null | grep '\.routes(routes!' )
}

check_pubmod_present() {
  local file="$1"
  local branch="$2"

  while IFS= read -r line; do
    [[ "$line" =~ ^pub\ mod ]] || continue
    if ! git show "origin/$branch:$file" 2>/dev/null | grep -qF "$line"; then
      echo "MISSING in $branch/$file: $line"
      ERRORS=$((ERRORS + 1))
    fi
  done < <(git show "origin/main:$file" 2>/dev/null)
}

check_envkey_present() {
  local file="$1"
  local branch="$2"

  while IFS= read -r line; do
    [[ "$line" =~ ^# ]] && continue
    [[ -z "${line// }" ]] && continue
    if ! git show "origin/$branch:$file" 2>/dev/null | grep -qF "$line"; then
      echo "MISSING in $branch/$file: $line"
      ERRORS=$((ERRORS + 1))
    fi
  done < <(git show "origin/main:$file" 2>/dev/null)
}

echo "=== Checking branch: $BRANCH ==="
check_lines_present  "apps/backend/src/routes/auth.rs"    "$BRANCH"
check_pubmod_present "apps/backend/src/handlers/mod.rs"   "$BRANCH"
check_envkey_present "apps/backend/.env.example"          "$BRANCH"

echo ""
if [[ $ERRORS -eq 0 ]]; then
  echo "OK: no regressions detected in $BRANCH"
else
  echo "FAIL: $ERRORS regression(s) found in $BRANCH"
  exit 1
fi

#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
checker="$script_dir/check-internal-cmd-ids.sh"
repo_root=$(cd "$script_dir/../.." && pwd)

make_stub_repo() {
  local dir="$1"
  local rel_path="$2"
  local contents="$3"

  rm -rf "$dir"
  mkdir -p "$dir/$(dirname "$rel_path")"
  printf '%s\n' "$contents" >"$dir/$rel_path"
  git -C "$dir" init -q
  git -C "$dir" add "$rel_path"
}

expect_fail() {
  local label="$1"
  local dir="$2"

  if bash "$checker" "$dir"; then
    echo "expected the $label fixture to fail, but it passed" >&2
    exit 1
  fi
}

bash "$checker" "$repo_root"

grep -q 'subtask_count' "$repo_root/docs/features/tasks/1.core.md"
grep -q 'subtask_all_done' "$repo_root/docs/features/tasks/8.automation.md"
grep -q 'root_and_subtask_lists_apply_the_same_filters' \
  "$repo_root/apps/backend/tests/task_list_integration.rs"
bash "$checker" "$repo_root"

stub=$(mktemp -d)
trap 'rm -rf "$stub"' EXIT

make_stub_repo "$stub/cmd-leak" 'apps/leak.ts' 'const note = "see cmd_999 for context";'
expect_fail 'cmd leak' "$stub/cmd-leak"

make_stub_repo "$stub/subtask-leak" 'apps/leak.ts' 'const note = "from subtask_19_abc123";'
expect_fail 'subtask leak' "$stub/subtask-leak"

echo 'internal-cmd-id gate self-test passed (clean green, cmd/subtask leaks red, task vocabulary green)'

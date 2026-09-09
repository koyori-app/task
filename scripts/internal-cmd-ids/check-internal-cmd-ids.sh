#!/usr/bin/env bash
set -euo pipefail

root="${1:-.}"

if [[ ! -d "$root" ]]; then
  echo "internal-cmd-id gate: source directory not found: $root" >&2
  exit 2
fi

# パターンを literal 連結で組み立て、検査自身が自分を引っかけないようにする。
cmd_prefix='cmd'
subtask_prefix='subtask'
cmd_pattern="${cmd_prefix}_[0-9]+"
subtask_pattern="${subtask_prefix}_[0-9]+_[A-Za-z0-9]+"
combined_pattern="(${cmd_pattern}|${subtask_pattern})"

mapfile -d '' files < <(
  git -C "$root" ls-files -z \
    ':(glob)apps/**' \
    ':(glob)docs/**' \
    ':(glob)packages/**' \
    ':(glob)scripts/**' \
    ':(glob)e2e/**' \
    ':!scripts/internal-cmd-ids/**'
)

if [[ ${#files[@]} -eq 0 ]]; then
  echo "internal-cmd-id gate: no tracked files found under $root"
  exit 0
fi

violations=0
for file in "${files[@]}"; do
  if grep -En "$combined_pattern" "$root/$file" 2>/dev/null; then
    echo "$file: internal command id leaked into tracked source/docs" >&2
    violations=1
  fi
done

if [[ $violations -ne 0 ]]; then
  cat >&2 <<'EOF'
internal-cmd-id gate failed.
Remove internal command ids (cmd_<n>, subtask_<n>_<token>) from comments and docs.
Describe the reason itself instead of the ledger pointer.
EOF
  exit 1
fi

echo "internal-cmd-id gate passed"

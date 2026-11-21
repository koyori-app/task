#!/usr/bin/env bash
# CI と同じ検証をまとめて回し、段ごとに 1 行で結果を出す。全文のログは .verify/<段>.log に残す。
#
# 使い方: scripts/verify.sh [changed|all|backend|cli|frontend|docs]...（既定は changed）
#   changed   main との差分（コミット済み + 作業ツリー）から要る区分だけを回す
#   backend   fmt・hakari・clippy・テスト（統合テストを含む）・openapi.json のずれ
#   cli       task-cli のテスト
#   frontend  openapi の型生成・fmt・lint・未使用の検出・型検査・単体テスト・API パスの引数の検査
#   docs      仕様書の文体の lint とそのテスト
# 環境変数: VERIFY_BASE（changed の比較元。既定は origin/main との merge-base）
#           VERIFY_LOG_DIR（ログの置き場。既定は <リポジトリ>/.verify）
# 失敗した段があっても最後まで回し、最後に失敗した段のログの末尾を出して 1 で終える。
set -uo pipefail

root=$(git rev-parse --show-toplevel)
log_dir=${VERIFY_LOG_DIR:-$root/.verify}
mkdir -p "$log_dir"
failed=()

# step <名前> <ディレクトリ> <コマンド...>
step() {
  local name=$1 dir=$2
  shift 2
  local log="$log_dir/$name.log" start=$SECONDS
  if (cd "$root/$dir" && "$@") >"$log" 2>&1; then
    printf 'ok    %-18s %4ss\n' "$name" $((SECONDS - start))
  else
    printf 'FAIL  %-18s %4ss  %s\n' "$name" $((SECONDS - start)) "$log"
    failed+=("$name")
  fi
}

skip() { printf 'skip  %-18s       %s\n' "$1" "$2"; }

has_nextest() { cargo nextest --version >/dev/null 2>&1; }

openapi_drift() {
  local exported normalized committed
  exported=$(mktemp) normalized=$(mktemp) committed=$(mktemp)
  cargo run --quiet --bin export_openapi -- "$exported" &&
    jq -S '.' "$root/apps/frontend/openapi.json" >"$committed" &&
    jq -S '.' "$exported" >"$normalized" &&
    diff -u "$committed" "$normalized"
  local status=$?
  rm -f "$exported" "$normalized" "$committed"
  [ "$status" -eq 0 ] || echo "openapi.json がずれている: cd apps/frontend && pnpm openapi && node_modules/.bin/vp fmt"
  return "$status"
}

run_backend() {
  step backend-fmt apps/backend cargo fmt --all -- --check
  if cargo hakari --version >/dev/null 2>&1; then
    step hakari apps/backend sh -c 'cargo hakari generate --diff && cargo hakari manage-deps --dry-run'
  else
    skip hakari "cargo-hakari が無い（cargo install cargo-hakari --locked）"
  fi
  step clippy apps/backend cargo clippy --workspace --all-targets -- -D warnings
  if has_nextest; then
    step backend-test apps/backend cargo nextest run --test-threads=1
  else
    step backend-test apps/backend cargo test -- --test-threads=1
  fi
  step openapi-drift apps/backend openapi_drift
}

run_cli() {
  if has_nextest; then
    step cli-test apps/backend cargo nextest run -p task-cli
  else
    step cli-test apps/backend cargo test -p task-cli
  fi
}

run_frontend() {
  if [ ! -d "$root/apps/frontend/node_modules" ]; then
    printf 'FAIL  %-18s       apps/frontend で pnpm install --frozen-lockfile を先に実行する\n' frontend
    failed+=(frontend)
    return
  fi
  step openapi-types apps/frontend pnpm run openapi:generate
  step frontend-fmt apps/frontend pnpm fmt:check
  step frontend-lint apps/frontend pnpm lint
  step knip apps/frontend pnpm knip
  step typecheck apps/frontend pnpm vue-tsc --noEmit
  step unit apps/frontend pnpm test:unit
  step api-path-params . bash scripts/api-path-params/check-api-path-params.sh apps/frontend/src
}

run_docs() {
  step docs-style . sh -c 'node docs/scripts/lint-style.mjs && node --test docs/scripts/lint-style.test.mjs'
}

changed_groups() {
  local base files
  base=${VERIFY_BASE:-$(git merge-base origin/main HEAD)}
  files=$({
    git diff --name-only "$base" HEAD
    git diff --name-only HEAD
    git ls-files --others --exclude-standard
  } | sort -u)
  local groups=()
  # backend の変更は payload の型を使う CLI にも効く
  grep -q '^apps/backend/' <<<"$files" && groups+=(backend cli)
  grep -qE '^(apps/frontend/|scripts/api-path-params/)' <<<"$files" && groups+=(frontend)
  grep -q '^docs/' <<<"$files" && groups+=(docs)
  [ ${#groups[@]} -eq 0 ] || printf '%s\n' "${groups[@]}"
}

targets=("$@")
[ ${#targets[@]} -eq 0 ] && targets=(changed)
groups=()
for t in "${targets[@]}"; do
  case $t in
    changed) mapfile -t -O "${#groups[@]}" groups < <(changed_groups) ;;
    all) groups+=(backend cli frontend docs) ;;
    backend | cli | frontend | docs) groups+=("$t") ;;
    -h | --help)
      sed -n '2,14p' "$0"
      exit 0
      ;;
    *)
      echo "不明な区分: $t（changed / all / backend / cli / frontend / docs）" >&2
      exit 2
      ;;
  esac
done

seen=" "
ran=0
for g in "${groups[@]}"; do
  case $seen in *" $g "*) continue ;; esac
  seen+="$g "
  ran=1
  "run_$g"
done
[ "$ran" -eq 1 ] || echo "回す区分が無い（main との差分に backend・frontend・docs の変更が無い）"

if [ ${#failed[@]} -gt 0 ]; then
  for name in "${failed[@]}"; do
    [ -f "$log_dir/$name.log" ] || continue
    printf '\n== %s（%s の末尾）\n' "$name" "$log_dir/$name.log"
    tail -n 20 "$log_dir/$name.log"
  done
  printf '\n失敗: %s\n' "${failed[*]}"
  exit 1
fi

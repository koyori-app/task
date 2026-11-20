#!/usr/bin/env bash
# CI と同じ検証をまとめて回し、段ごとに 1 行で結果を出す。全文のログは .verify/<段>.log に残す。
#
# 使い方: scripts/verify.sh [changed|all|backend|cli|frontend|docs]...（既定は changed）
#   changed   main との差分（コミット済み + 作業ツリー）から要る区分だけを回す
#   backend   fmt・hakari・clippy・テスト（統合テストを含む）・openapi.json のずれ
#   cli       task-cli のテスト
#   frontend  openapi の型生成・fmt・lint・型検査・単体テスト・API パスの引数の検査
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

# 統合テスト用の Postgres と Valkey を 1 組起こし、行き先を TEST_DATABASE_URL / TEST_REDIS_URL に置く。
# ハーネスはこの行き先に試験ごとの DB を作るので、nextest を並列に回せる。
# 行き先が片方でも渡されているなら、それを使って何も起こさない（足りない側はハーネスが起こす）。
# docker が無い・起動に失敗したときは 1 行出して戻り、ハーネスが試験ごとにコンテナを起こす。
verify_containers=()
verify_exported=0

# 自分が起こしたコンテナを消す。消せなかった名は控えたままにして 1 を返す（後でもう一度試せる）。
stop_test_services() {
  # 消したコンテナの行き先を後の段へ渡さない
  if [ "$verify_exported" -eq 1 ]; then
    unset TEST_DATABASE_URL TEST_REDIS_URL
    verify_exported=0
  fi
  [ ${#verify_containers[@]} -gt 0 ] || return 0
  local name left=()
  for name in "${verify_containers[@]}"; do
    # 無い名の rm -f は成功する。失敗は docker に届かないなどで、コンテナが残っているかもしれない
    docker rm -f -v "$name" >/dev/null 2>&1 || left+=("$name")
  done
  verify_containers=("${left[@]}")
  [ ${#left[@]} -eq 0 ] && return 0
  echo "verify: コンテナを消せなかった: ${left[*]}（docker rm -f -v ${left[*]}）" >&2
  return 1
}

wait_test_service() {
  local i
  for i in $(seq 1 60); do
    docker exec "$@" >/dev/null 2>&1 && return 0
    sleep 0.5
  done
  return 1
}

start_test_services() {
  [ -n "${TEST_DATABASE_URL:-}${TEST_REDIS_URL:-}" ] && return 0
  if ! command -v docker >/dev/null 2>&1; then
    echo "verify: docker が無いので、統合テスト用の共有コンテナを起こさない" >&2
    return 0
  fi
  local pg="task-verify-pg-$$" valkey="task-verify-valkey-$$" pg_port valkey_port
  # イメージはハーネス（tests/common/mod.rs）と同じ
  # 途中で失敗しても消せるよう、起こす前に名を控える（無い名の rm -f は何もしない）
  verify_containers=("$pg" "$valkey")
  # 初期化中の Postgres は TCP を聞かないので、-h を付けて本起動を待つ
  # 既定の 100 接続では 12 並列で「too many clients」に当たる（試験のプロセスごとにプールを持つ）。
  # 16 並列ぶんを見込んで上限を上げる
  if docker run -d --name "$pg" -e POSTGRES_PASSWORD=postgres -p 127.0.0.1::5432 \
    postgres:17 -c fsync=off -c max_connections=500 >/dev/null 2>&1 &&
    docker run -d --name "$valkey" -p 127.0.0.1::6379 valkey/valkey:8.1 >/dev/null 2>&1 &&
    wait_test_service "$pg" pg_isready -h 127.0.0.1 -U postgres &&
    wait_test_service "$valkey" valkey-cli ping &&
    pg_port=$(docker port "$pg" 5432/tcp | head -n 1) && [ -n "$pg_port" ] &&
    valkey_port=$(docker port "$valkey" 6379/tcp | head -n 1) && [ -n "$valkey_port" ]; then
    export TEST_DATABASE_URL="postgresql://postgres:postgres@127.0.0.1:${pg_port##*:}/postgres"
    export TEST_REDIS_URL="redis://127.0.0.1:${valkey_port##*:}"
    verify_exported=1
    return 0
  fi
  echo "verify: 統合テスト用の共有コンテナを起こせなかった。試験ごとのコンテナで回す（遅い）" >&2
  stop_test_services
}
trap stop_test_services EXIT

# 試験ごとにコンテナを起こす経路で同時に回す数の上限（コンテナは 1 件につき最大 2 個）
PER_TEST_CONTAINER_THREADS=6

# nextest の並列数。Valkey の DB は 16 個なので 16 で頭打ちにする（macOS に nproc は無い）。
# 行き先が両方そろっていなければ、ハーネスが試験ごとにコンテナを起こすので、数を抑える。
test_threads() {
  local n
  n=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)
  [ "$n" -gt 16 ] && n=16
  if [ -z "${TEST_DATABASE_URL:-}" ] || [ -z "${TEST_REDIS_URL:-}" ]; then
    [ "$n" -gt "$PER_TEST_CONTAINER_THREADS" ] && n=$PER_TEST_CONTAINER_THREADS
  fi
  echo "$n"
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
    # step の中はサブシェルなので、行き先の export はここ（親）で済ませる
    start_test_services
    step backend-test apps/backend cargo nextest run --test-threads="$(test_threads)"
    stop_test_services
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

# 段の後で消せなかったコンテナをもう一度試す。残れば失敗として終える
if ! stop_test_services; then
  printf 'FAIL  %-18s       残ったコンテナ: %s\n' test-services "${verify_containers[*]}"
  failed+=(test-services)
fi

if [ ${#failed[@]} -gt 0 ]; then
  for name in "${failed[@]}"; do
    [ -f "$log_dir/$name.log" ] || continue
    printf '\n== %s（%s の末尾）\n' "$name" "$log_dir/$name.log"
    tail -n 20 "$log_dir/$name.log"
  done
  printf '\n失敗: %s\n' "${failed[*]}"
  exit 1
fi

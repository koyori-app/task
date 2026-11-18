#!/usr/bin/env bash
# CHANGELOG.md とリリースノートを git-cliff（cliff.toml）で作る。
#
# 使い方: scripts/changelog.sh write [<tag>]   CHANGELOG.md を <tag> までの内容で書き直す（既定は最新の版のタグ）。
#                                              版のタグでないもの（v0.1.53-rc.1 など）は拒む
#         scripts/changelog.sh notes [<tag>]   <tag> のリリースノートを標準出力へ出す（既定は最新の版のタグ）。
#                                              まだ無いタグや版のタグでないものを渡すと、最後の版の
#                                              後ろの分（未リリースの分）をその名前の版として出す
#         scripts/changelog.sh check           cliff.toml の固定の検査と、CHANGELOG.md のずれの検査
#         scripts/changelog.sh latest          最新の版のタグを出す（GitHub へ取りに行かない）
# 版のタグは cliff.toml の tag_pattern（vX.Y.Z）に当たるものだけ。ここと changelog.yml はそれを読む
# 環境変数: GIT_CLIFF（git-cliff の実行ファイル。既定は PATH の git-cliff、無ければ npx で版を固定して呼ぶ）
#           CHANGELOG_PR_MAP（PR の一覧の TSV。渡せば write / notes は GitHub へ取りに行かない。
#           check は渡しても、closes.md を GitHub に描画させるので認証とネットワークが要る）
# マージ済みの PR の一覧を GitHub から取るので、gh と、その認証（gh auth login か GH_TOKEN）が要る。
# 要るコマンドは gh・jq（check）・python3。一覧を表にするのは scripts/changelog_pr_map.py
# （標準ライブラリだけ）。
set -euo pipefail

# release-cli.yml と changelog.yml の tool: git-cliff@<版> と揃える
GIT_CLIFF_VERSION=2.14.2
REPO_OWNER=koyori-app
REPO_NAME=task

root=$(git rev-parse --show-toplevel)
cd "$root"

cliff() {
  if [ -n "${GIT_CLIFF:-}" ]; then
    "$GIT_CLIFF" --config "$root/cliff.toml" "$@"
  elif command -v git-cliff >/dev/null 2>&1; then
    git-cliff --config "$root/cliff.toml" "$@"
  else
    npx --yes "git-cliff@$GIT_CLIFF_VERSION" --config "$root/cliff.toml" "$@"
  fi
}

# マージ済みの PR を「マージコミットの SHA・番号・題」の TSV にして CHANGELOG_PR_MAP へ置く。
# スタック PR は下の PR も上の PR のマージコミットを指すので、cliff.toml がこの表で 1 コミットを
# PR の数だけの項目に広げる。PR の本文の Closes #N と Closes TASK-N（Fixes / Resolves も。大文字小文字を
# 問わない）は、題に無いものだけを「（#N、TASK-N を解決）」として題に足す（cliff.toml が Issue と
# タスクへのリンクにする）。Closes 等の付かない TASK-N の言及は載せない。
# 本文は Markdown のまま読まず、GitHub が描画した HTML（bodyHTML）を changelog_pr_map.py が
# HTML のパーサーで読み、コード・引用・取り消し線の要素の外の地の文だけから拾う。
# フェンスの種類や字下げなど Markdown の書き方の差は GitHub の描画に任せる（正規表現で除かない）。
# 取れなければ止める（表が無いと下の PR が黙って抜けた CHANGELOG ができる）
fetch_pr_map() {
  [ -z "${CHANGELOG_PR_MAP:-}" ] || return 0
  local pulls
  CHANGELOG_PR_MAP=$(mktemp)
  pulls=$(mktemp)
  # trap は関数の外で走るので、パスをここで展開して渡す
  # shellcheck disable=SC2064
  trap "rm -f '$CHANGELOG_PR_MAP' '$pulls'" EXIT
  # shellcheck disable=SC2016
  gh api graphql --paginate -F owner="$REPO_OWNER" -F name="$REPO_NAME" -f query='
    query($owner: String!, $name: String!, $endCursor: String) {
      repository(owner: $owner, name: $name) {
        pullRequests(states: MERGED, first: 100, after: $endCursor, orderBy: {field: CREATED_AT, direction: ASC}) {
          pageInfo { hasNextPage endCursor }
          nodes { number title bodyHTML mergeCommit { oid } }
        }
      }
    }' >"$pulls" ||
    { echo "PR の一覧を取れない（gh auth login か GH_TOKEN を確かめる）" >&2; exit 1; }
  python3 -B "$root/scripts/changelog_pr_map.py" <"$pulls" >"$CHANGELOG_PR_MAP"
  [ -s "$CHANGELOG_PR_MAP" ] || { echo "PR の一覧が空" >&2; exit 1; }
  export CHANGELOG_PR_MAP
}

# 版のタグ（cliff.toml の tag_pattern）だけを扱う。v0.1.53-rc.1 などは版と見なさない。
# 形を cliff.toml の 1 か所に置き、ここで読む
VERSION_TAG=$(sed -n "s/^tag_pattern = '\(.*\)'\$/\1/p" "$root/cliff.toml")
[ -n "$VERSION_TAG" ] || { echo "cliff.toml に tag_pattern が無い" >&2; exit 1; }
is_version_tag() { grep -qE "$VERSION_TAG" <<<"$1"; }
version_tags() { git tag -l 'v*' "$@" | { grep -E "$VERSION_TAG" || true; }; }

latest_tag() { version_tags --sort=-v:refname | head -1; }

# 最初のコミット。git-cliff は範囲の終わりだけを渡せないので、始まりに使う
first_commit() { git rev-list --max-parents=0 HEAD | tail -1; }

# <tag> の 1 つ前の版のタグ。無ければ空
previous_tag() {
  version_tags --sort=v:refname | awk -v tag="$1" '$0 == tag { print prev; exit } { prev = $0 }'
}

write() {
  local tag=${1:-$(latest_tag)}
  # 版でないタグまでで作ると「未リリース」の節が入り、ずれの検査が落ち続ける
  is_version_tag "$tag" || { echo "版のタグ（vX.Y.Z）でない: $tag" >&2; exit 1; }
  cliff --output "$root/CHANGELOG.md" "$(first_commit)..$tag"
}

notes() {
  local tag=${1:-$(latest_tag)} prev
  # 版でないタグ（rc など）は、まだ無いタグと同じく、最後の版の後ろの分をその名前で出す
  if is_version_tag "$tag" && git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    prev=$(previous_tag "$tag")
    cliff --strip header "${prev:-$(first_commit)}..$tag"
  else
    cliff --strip header --unreleased --tag "$tag"
  fi
}

fail() { echo "changelog check: $*" >&2; exit 1; }

# 項目の行（PR へのリンクで終わる箇条書き）
ITEM='^- .*\(\[#[0-9]+\]\(https://github\.com/koyori-app/task/pull/[0-9]+\)\)$'

# PR の本文から解決の参照を拾う部分（changelog_pr_map.py）の単体テスト。
# 保存した描画結果（changelog-fixtures/closes.html）に加えて、同じ本文（closes.md）を今の GitHub に
# 描画させた結果でも同じ参照になることを見る（GitHub の描画が変わったらここで気づく）
check_pr_map() {
  local fixtures="$root/scripts/changelog-fixtures" rendered
  rendered=$(mktemp)
  jq -n --rawfile text "$fixtures/closes.md" '{text: $text, mode: "gfm", context: "koyori-app/task"}' |
    gh api markdown --input - >"$rendered" || { rm -f "$rendered"; fail "closes.md を GitHub に描画させられない"; }
  CHANGELOG_RENDERED_FIXTURE="$rendered" python3 -B "$root/scripts/changelog_pr_map_test.py" ||
    { rm -f "$rendered"; fail "changelog_pr_map.py のテストが落ちた"; }
  rm -f "$rendered"
}

check() {
  local out tag expected
  check_pr_map
  # 履歴は変わらないので、過去のリリースの出力を固定して cliff.toml の退行を見る
  out=$(notes v0.1.52)
  [ "$(grep -cE -- "$ITEM" <<<"$out")" -eq 14 ] || fail "v0.1.52 の項目が 14 件でない"
  [ "$(grep -c '^- ' <<<"$out")" -eq 14 ] || fail "v0.1.52 に PR へのリンクの無い項目がある"
  grep -q '^## \[0\.1\.52\](https://github.com/koyori-app/task/compare/v0\.1\.51\.\.\.v0\.1\.52) - 2026-11-17$' <<<"$out" ||
    fail "v0.1.52 の見出しが違う"
  grep -q '^タスクに種別を持たせ' <<<"$out" || fail "タグのメッセージが出ていない"
  # マージコミットの PR。TASK-N はタスクへ、(#N) は PR へのリンクになる
  grep -qF -- '- タスクに種別を持たせる ([TASK-509](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-509)) ([#1199](https://github.com/koyori-app/task/pull/1199))' <<<"$out" ||
    fail "マージコミットの PR（#1199）が出ていない"
  # スタック PR の下の PR（#1192 のマージコミットで入った #1188・#1189・#1190）
  for n in 1188 1189 1190 1192; do
    grep -qF "([#$n](https://github.com/koyori-app/task/pull/$n))" <<<"$out" || fail "スタック PR（#$n）が出ていない"
  done
  # PR の本文の Closes TASK-N（#1187 の TASK-489・TASK-523）は、題に無い分だけを解決としてタスクへのリンクにする
  grep -qF -- '([TASK-533](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-533))（[TASK-489](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-489)、[TASK-523](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-523) を解決） ([#1187](https://github.com/koyori-app/task/pull/1187))' <<<"$out" ||
    fail "PR の本文の Closes TASK-N（#1187 の TASK-489・TASK-523）が出ていない"
  # Closes の付かない本文の言及（#1187 の TASK-532、#1192 の TASK-535）は載せない
  grep -q 'TASK-532\|TASK-535' <<<"$out" && fail "PR の本文で言及しただけの TASK-N が出ている"
  # 題にも本文の Closes にもある TASK-N（#1199 の TASK-509）は 1 度だけ出す
  [ "$(grep -o '\[TASK-509\]' <<<"$out" | wc -l)" -eq 1 ] || fail "題と Closes にある TASK-509 が 1 度でない"
  grep -q '(deps)\|#1185\]\|#875\]' <<<"$out" && fail "依存の更新が出ている"
  grep -q 'レビュー指摘' <<<"$out" && fail "PR の中のコミットが出ている"
  # squash の PR（#1113）と、本文の無いマージコミット（#741）
  # grep -q は見つけた所で読むのをやめる。pipefail で書き手の SIGPIPE を失敗に数えないよう、出力は変数に受ける
  out=$(notes v0.1.48)
  grep -qF '([#1113](https://github.com/koyori-app/task/pull/1113))' <<<"$out" || fail "squash の PR（#1113）が出ていない"
  out=$(notes v0.1.24)
  grep -qF '([#741](https://github.com/koyori-app/task/pull/741))' <<<"$out" || fail "本文の無いマージコミット（#741）が出ていない"
  # 題の中の #N（#620 の題の #317）と、PR の本文の Closes #N（#684 の #683）は Issue へのリンクになる
  out=$(notes v0.1.9)
  grep -qF '([#317](https://github.com/koyori-app/task/issues/317)) ([#620](https://github.com/koyori-app/task/pull/620))' <<<"$out" ||
    fail "題の中の Issue（#317）がリンクになっていない"
  out=$(notes v0.1.16)
  grep -qF '（[#683](https://github.com/koyori-app/task/issues/683) を解決） ([#684](https://github.com/koyori-app/task/pull/684))' <<<"$out" ||
    fail "PR の本文の Closes（#683）が出ていない"
  out=$(notes v0.1.0)
  # 引用とコードの中の Closes（#350 の本文は #351 の `Closes #309` を説明しているだけ）は拾わない。
  # 対照として、行頭に Closes #309 を持つ #351 は解決として載せる
  grep -qF -- '- **frontend**: create tasks from the project view ([#350](https://github.com/koyori-app/task/pull/350))' <<<"$out" ||
    fail "引用の中の Closes（#350 の #309）を解決として拾っている"
  grep -qF '（[#309](https://github.com/koyori-app/task/issues/309) を解決） ([#351](https://github.com/koyori-app/task/pull/351))' <<<"$out" ||
    fail "PR の本文の Closes（#351 の #309）が出ていない"
  # 題にも本文の Closes にもある #N（#278 の #151）は題の側だけを残す
  grep -qF -- '- **workspace**: service / job / handler クレートへ分割 ([#151](https://github.com/koyori-app/task/issues/151) Phase 4) ([#278](https://github.com/koyori-app/task/pull/278))' <<<"$out" ||
    fail "題と Closes にある #151（#278）が 1 度でない"
  # 版のタグは vX.Y.Z だけ（rc などを最新の版や前の版にしない）
  is_version_tag v0.1.52 || fail "v0.1.52 を版のタグと見ていない"
  is_version_tag v0.1.53-rc.1 && fail "v0.1.53-rc.1 を版のタグと見ている"
  [ "$(previous_tag v0.1.52)" = v0.1.51 ] || fail "v0.1.52 の前の版が v0.1.51 でない"
  # cliff.toml のリンクの境界は ASCII の英数字で見る（日本語に接した参照もリンクにし、色の #1e1e1e はリンクにしない）
  out=$(cliff --strip header --unreleased --with-commit 'feat: これでTASK-5、TASK-509の対応、#12の件、色 #1e1e1e (#1)')
  grep -qF -- '- これで[TASK-5](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-5)、[TASK-509](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-509)の対応、[#12](https://github.com/koyori-app/task/issues/12)の件、色 #1e1e1e ([#1](https://github.com/koyori-app/task/pull/1))' <<<"$out" ||
    fail "日本語に接した TASK-N・#N のリンクが違う"

  # CHANGELOG.md は、先頭の版までを生成し直した結果と一致する
  [ -f "$root/CHANGELOG.md" ] || fail "CHANGELOG.md が無い: scripts/changelog.sh write"
  tag=v$(sed -n 's/^## \[\([0-9.]*\)\].*/\1/p' "$root/CHANGELOG.md" | head -1)
  [ "$tag" != v ] || fail "CHANGELOG.md に版の見出しが無い"
  expected=$(mktemp)
  cliff --output "$expected" "$(first_commit)..$tag"
  diff -u "$root/CHANGELOG.md" "$expected" ||
    { rm -f "$expected"; fail "CHANGELOG.md がずれている: scripts/changelog.sh write $tag"; }
  rm -f "$expected"
  echo "changelog check: ok（CHANGELOG.md は $tag まで）"
}

case ${1:-} in
  write) fetch_pr_map; write "${2:-}" ;;
  notes) fetch_pr_map; notes "${2:-}" ;;
  check) fetch_pr_map; check ;;
  latest) latest_tag ;;
  *) awk 'NR > 1 && /^set /{ exit } NR > 1' "$0" >&2; exit 2 ;;
esac

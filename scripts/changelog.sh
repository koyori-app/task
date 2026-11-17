#!/usr/bin/env bash
# CHANGELOG.md とリリースノートを git-cliff（cliff.toml）で作る。
#
# 使い方: scripts/changelog.sh write [<tag>]   CHANGELOG.md を <tag> までの内容で書き直す（既定は最新の v* タグ）
#         scripts/changelog.sh notes [<tag>]   <tag> のリリースノートを標準出力へ出す（既定は最新の v* タグ）。
#                                              まだ無いタグを渡すと、未リリースの分をその版として出す
#         scripts/changelog.sh check           cliff.toml の固定の検査と、CHANGELOG.md のずれの検査
# 環境変数: GIT_CLIFF（git-cliff の実行ファイル。既定は PATH の git-cliff、無ければ npx で版を固定して呼ぶ）
#           CHANGELOG_PR_MAP（PR の一覧の TSV。渡せば GitHub へ取りに行かない）
# マージ済みの PR の一覧を GitHub から取るので、gh と、その認証（gh auth login か GH_TOKEN）が要る。
set -euo pipefail

# release-cli.yml と changelog.yml の tool: git-cliff@<版> と揃える
GIT_CLIFF_VERSION=2.14.2
REPO=koyori-app/task

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
# 取れなければ止める（表が無いと下の PR が黙って抜けた CHANGELOG ができる）
fetch_pr_map() {
  [ -z "${CHANGELOG_PR_MAP:-}" ] || return 0
  local pulls
  CHANGELOG_PR_MAP=$(mktemp)
  pulls=$(mktemp)
  # trap は関数の外で走るので、パスをここで展開して渡す
  # shellcheck disable=SC2064
  trap "rm -f '$CHANGELOG_PR_MAP' '$pulls'" EXIT
  gh api --paginate "repos/$REPO/pulls?state=closed&per_page=100" >"$pulls" ||
    { echo "PR の一覧を取れない（gh auth login か GH_TOKEN を確かめる）" >&2; exit 1; }
  jq -r 'def closes($ref): [(.body // "")
        | scan("(?i)\\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?):?\\s+" + $ref + "([0-9]+)\\b") | .[0] | tonumber]
      | unique | map(tostring);
    .[] | select(.merged_at != null) | .title as $title
    | ((closes("#") | map(select(. as $n | $title | test("#" + $n + "\\b") | not)) | map("#" + .))
      + (closes("TASK-") | map(select(. as $n | $title | test("TASK-" + $n + "\\b"; "i") | not)) | map("TASK-" + .)))
      as $closes
    | [ .merge_commit_sha, .number,
        ($title | gsub("[\\t\\n\\r]"; " "))
          + (if ($closes | length) > 0 then "（" + ($closes | join("、")) + " を解決）" else "" end)
      ] | @tsv' "$pulls" | sort -t$'\t' -k2,2n >"$CHANGELOG_PR_MAP"
  [ -s "$CHANGELOG_PR_MAP" ] || { echo "PR の一覧が空" >&2; exit 1; }
  export CHANGELOG_PR_MAP
}

latest_tag() { git tag -l 'v*' --sort=-v:refname | head -1; }

# 最初のコミット。git-cliff は範囲の終わりだけを渡せないので、始まりに使う
first_commit() { git rev-list --max-parents=0 HEAD | tail -1; }

# <tag> の 1 つ前の v* タグ。無ければ空
previous_tag() {
  git tag -l 'v*' --sort=v:refname | awk -v tag="$1" '$0 == tag { print prev; exit } { prev = $0 }'
}

write() {
  local tag=${1:-$(latest_tag)}
  cliff --output "$root/CHANGELOG.md" "$(first_commit)..$tag"
}

notes() {
  local tag=${1:-$(latest_tag)} prev
  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    prev=$(previous_tag "$tag")
    cliff --strip header "${prev:-$(first_commit)}..$tag"
  else
    cliff --strip header --unreleased --tag "$tag"
  fi
}

fail() { echo "changelog check: $*" >&2; exit 1; }

# 項目の行（PR へのリンクで終わる箇条書き）
ITEM='^- .*\(\[#[0-9]+\]\(https://github\.com/koyori-app/task/pull/[0-9]+\)\)$'

check() {
  local out tag expected
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
  *) sed -n '2,10p' "$0" >&2; exit 2 ;;
esac

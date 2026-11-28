# task CLI

`task` コマンド。Rust の単一バイナリで、実行にランタイムの用意は要らない。

主なユースケースは AI レビュワーと CI からの `task review submit` / `task review summary`
（マージ前ゲート）。出し直した PR へ未解決の指摘を写す（元は変えない）操作は
`task review carry-over --project TASK --from-pr 790 --to-pr 1119`。
出した指摘を後から直すときは `task review edit <finding-id> --project TASK --title …`
（直せるのはその指摘のラウンドの作成者だけで、`open` の間だけ。重大度は直せない）。
コマンドの仕様は `docs/dev/features/review-findings/index.md` の「CLI」が正。

## 導入

`v*` タグを push すると GitHub Release にバイナリが添付される
（`.github/workflows/release-cli.yml`）。clone もビルドも要らない。

| 配布物 | 使う場面 |
|---|---|
| `task-<version>-x86_64-unknown-linux-musl.tar.gz` | Linux 全般。静的リンクなので alpine 等の CI コンテナでもそのまま動く |
| `task-<version>-x86_64-unknown-linux-gnu.tar.gz` | glibc のある Linux |
| `task-<version>-aarch64-apple-darwin.tar.gz` | Apple Silicon の macOS |
| `task-<version>-x86_64-pc-windows-msvc.zip` | Windows |

GitHub Actions から使う例。取り違えや破損に気づけるよう `.sha256` を突き合わせる。

```yaml
- name: Install the task CLI
  env:
    # gh は Actions 上でトークンを明示しないと動かない
    GH_TOKEN: ${{ github.token }}
    TASK_CLI_VERSION: v0.1.9
  run: |
    set -euo pipefail
    asset="task-${TASK_CLI_VERSION#v}-x86_64-unknown-linux-musl.tar.gz"
    gh release download "$TASK_CLI_VERSION" --repo koyori-app/task --pattern "$asset*"
    sha256sum --check "${asset}.sha256"

    # runner のユーザーは /usr/local/bin へ書けない。書ける場所へ展開して PATH へ通す
    mkdir -p "$RUNNER_TEMP/bin"
    tar -xzf "$asset" -C "$RUNNER_TEMP/bin"
    "$RUNNER_TEMP/bin/task" --version
    echo "$RUNNER_TEMP/bin" >> "$GITHUB_PATH"
```

`GITHUB_PATH` が効くのは次のステップからなので、同じステップの中では絶対パスで呼ぶ。

手元でも同じ手順で入る。

```bash
version=v0.1.9
asset="task-${version#v}-x86_64-unknown-linux-musl.tar.gz"
gh release download "$version" --repo koyori-app/task --pattern "$asset*"
sha256sum --check "${asset}.sha256"   # macOS は shasum -a 256 --check
mkdir -p ~/.local/bin && tar -xzf "$asset" -C ~/.local/bin
~/.local/bin/task --version
```

`--version` が名乗る版はタグと一致する（リリース時にタグから注入し、その場で
突き合わせている）。タグ以外でビルドしたバイナリはクレートの版を名乗る。

### 更新

新しい版があると、コマンドの最後に標準エラーへ 1 行で知らせる
（`A new release of task is available: 0.1.51 -> 0.2.0. Run \`task self-update\` to install it.`）。
標準出力と終了コードは変えない。

- 確認は 1 日に 1 回まで（最後に確かめた時刻を `~/.config/task/update-check.json` に置く）。待ち時間は最大 2 秒
- `TASK_NO_UPDATE_CHECK=1` で止まる。`CI` が空でなければ出ない。タグ以外でビルドしたバイナリ（版が `0.1.0`）も出ない
- 通信・GitHub の制限・応答の形の失敗は黙って無視する。本体のコマンドは止まらない

```bash
task self-update   # 最新の配布物を落とし、.sha256 を確かめて、今の task と入れ替える
```

`self-update` が確かめるのは `.sha256`（破損や取り違え）までで、配布元の確かさは TLS に頼る。署名の検証は
下の「検め方」の手順で行う。Windows は非対応（動いている `.exe` は置き換えられないので、`.zip` を落として入れ替える）。
GitHub の API を使うので、`task` のトークンは送らない。
タグ以外でビルドしたバイナリ（手元のビルド）では、実行ファイルを置き換えず、案内を出して終了コード 1 で止まる。
新しい実行ファイルは、元の実行ファイルの権限を引き継ぐ。

### 検め方

リリースには、四つの配布物の hash を束ねた `SHA256SUMS` と、それへの keyless 署名
`SHA256SUMS.cosign.bundle` も添えてある。`.sha256` の突き合わせで分かるのは
「壊れていないか」まで。署名の検証まで行うと「この repo の release workflow が
タグから作った物か」まで確かめられる。

[cosign](https://github.com/sigstore/cosign) の **v3 以上**が要る。

```bash
version=v0.1.9
gh release download "$version" --repo koyori-app/task --pattern "SHA256SUMS*"

# 署名を検める。--certificate-identity-regexp と --certificate-oidc-issuer を
# 省いてはならない。省くと「誰かが Sigstore で署名した」ことしか確かめておらず、
# 別人が作った SHA256SUMS でも通ってしまう
cosign verify-blob SHA256SUMS \
  --bundle SHA256SUMS.cosign.bundle \
  --certificate-identity-regexp '^https://github\.com/koyori-app/task/\.github/workflows/release-cli\.yml@refs/tags/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com

# 手元へ落とした配布物が SHA256SUMS と一致することを確かめる
sha256sum --check SHA256SUMS --ignore-missing   # macOS は shasum -a 256 --check --ignore-missing
```

### コンテナイメージの検め方

ghcr.io へ公開しているイメージ（`task-backend` / `task-frontend`、
`.github/workflows/publish-images.yml`）にも同じ流儀で keyless 署名が付いている。
署名は tag ではなく digest に付くが、tag を指して verify しても cosign が
digest へ解決して検めるので、そのまま使える。

こちらも [cosign](https://github.com/sigstore/cosign) の **v3 以上**が要る。

```bash
# 署名を検める。バイナリと同じく --certificate-identity-regexp と
# --certificate-oidc-issuer を省いてはならない
cosign verify ghcr.io/koyori-app/task-backend:0.2.0 \
  --certificate-identity-regexp '^https://github\.com/koyori-app/task/\.github/workflows/publish-images\.yml@refs/tags/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

frontend は image 名を `task-frontend` に読み替える。

## ビルド

`apps/backend` の Cargo ワークスペースのメンバーなので、`cargo fmt --all` /
`cargo check --workspace` / `cargo clippy --workspace` / `cargo test --workspace`
の対象に入る。

```bash
cargo build --release -p task-cli   # apps/backend/target/release/task
```

版を差し替えたいときだけ `TASK_CLI_VERSION` を渡す（`build.rs` が読む）。

## タスクを扱う

リポジトリに `.koyori-task.yaml` を置けば `--project` を省ける（[リポジトリの既定のプロジェクト](#リポジトリの既定のプロジェクトkoyori-taskyaml)）。

一覧・検索は既定で 50 件（検索は 20 件）ずつ返る。総件数と続きの有無は出力の最後に出る。

```bash
task tasks list --project TASK --page 2 --limit 100
task tasks list --project TASK --status Todo --label bug --sort deadline_asc
task tasks list --project TASK --type バグ     # 種別で絞る
task tasks list --project TASK --archived
task tasks list --project TASK --unassigned  # 担当者がいないタスクだけ
task tasks list --project TASK --json key,title,status  # 項目を選ぶ（本文などは出ない）
task tasks search --project TASK ページング
task tasks search --project TASK @alice      # alice を印でメンションした説明文にも当たる
task tasks show TASK-181                     # 説明文のメンションは @今の名 で出る
task tasks show TASK-181 --raw               # 保存されたまま（<@利用者ID> の印）で出す
task tasks comments TASK-181                 # コメントを古い順に読む（返信は親の下に字下げ）。`--json` は API の形のまま
task my list --open                          # 自分の担当のうち未完了だけ
task my list --open --page 2 --limit 100     # ページングは tasks list と同じ
```

`tasks list` は既定で 1 件 1 行の表（`キー<TAB>状態<TAB>タイトル<TAB>ラベル<TAB>更新日時`）を出し、本文は出さない。
`--json` は全項目（本文を含む）に `key` を足した形で出す。項目を選ぶときは `--json key,title,status`
（`--json=key,title,status` も同じ）と並べる。出力は `{ "tasks": [...], "total", "next_cursor" }` のままで、
各行が指定した項目だけを指定した順で持つ。選べる項目は `TaskResponse` の項目と、CLI が足す `key`・`url`・
`status`（状態の名前。`url` は `tasks create` と同じ条件で、引けなければ `null`）。誤った名前は選べる項目を
並べて終了コード 2 で止まる。項目を選べるのは `tasks list` だけで、ほかのコマンドに付けると終了コード 2 で
止まる（値なしの `--json` はどのコマンドでも今までどおり全部を出す）。

`tasks show` は説明文のメンションの印を `@今の名` に戻して出す。それを `tasks update --description` へ
そのまま戻すとメンションが素の文字になるので、書き換えて戻すときは `--raw` で印のまま取る（`--json` でも同じ）。
`tasks comment` は投稿用で、読むのは `tasks comments`。出力は作成者・日時・本文で、本文のメンションは
`@今の名` に戻す（`--json` も同じ。形と順は API のまま）。
`tasks show` は添付ファイル（`attachments`。名前・MIME・サイズ・コンテンツ URL）も出す。本体の取得コマンドはまだ無い。

`--status` / `--label` / `--type` / `--assignee` / `--milestone` / `--sprint` は名前または UUID で指せる。
名前は完全一致を優先し、それがなければ大文字小文字を無視して照合する。候補が複数あるときは
候補 ID を含むエラーを返すので、UUID で指定する。
綴りを外すと、そのプロジェクトで使える名前が並ぶ。担当者の候補一覧には `write:task`
が要るので、読み取りだけのトークンでは名前解決はできても候補は並ばない。
`--unassigned` は担当者が 1 人もいないタスクだけを返す。`--assignee` とは同時に使えない。

作成と更新は同じ綴りの項目を受ける。
更新は本体・ラベル差分・担当者置換を一度の API 呼び出しで反映し、途中失敗時はすべて取り消す。

```bash
task tasks create --project TASK --title 直す \
  --soft-deadline 2026-09-30 --estimate 90 --label bug --assignee yupix
task tasks update TASK-181 --progress 40 --sprint week-40 --add-label bug
task tasks update TASK-181 --clear-sprint --archive
task tasks update TASK-181 --type バグ        # 種別は 1 つだけ。外すときは --clear-type
```

`tasks create` は作ったタスクのキーと画面の URL を、`--json` なしなら `TASK-532<TAB>https://…/tasks/TASK-532` の
1 行で出す。`--json` は今までの項目の前に `key` と `url` を足した形で出す。`url` は `api_url` が
`<画面の URL>/api` の形で、かつトークンに `read:tenant` があるときだけ入る（無ければ `null`、1 行は
キーだけ）。URL の取得は作る前に済ませるので、取得に失敗したらタスクは作らない。

### 一括作成

```bash
task tasks bulk-create --project TASK tasks.json [--status Todo] [--json]
cat tasks.json | task tasks bulk-create --project TASK -
```

`FILE` は JSON 配列（`-` で標準入力。1〜100 件）。各要素は `tasks create` のフラグと同じ名前・値で、
UUID でなく名前で書ける。`description_file` は無く、未知のキーは断る。

```json
[
  { "title": "起票", "description": "本文", "priority": "high", "status": "Todo",
    "labels": ["bug"], "assignees": ["yupix"], "parent": "TASK-480",
    "soft_deadline": "2026-09-30", "hard_deadline": "2026-10-31", "planned_start": "2026-09-01",
    "estimate": 60, "progress": 0, "milestone": "v1", "sprint": "week-40", "type": "バグ" }
]
```

- 状態は要素の `status` を優先し、無い要素だけ `--status`、どちらも無ければプロジェクト既定
- 送信前に全要素の名前を解決する。1 つでも解決できなければ API を叩かず `tasks[i]: …`（`i` は 0 始まり）
  を出して終了コード 2 で終わる。解決の途中の認証・権限・通信の失敗は、位置を添えたうえで元の終了コード
  （3 / 4 / 1）のまま返す。ラベル等の一覧はプロジェクト単位で 1 回だけ取る
- API は全件作るか 1 件も作らないか。途中で落ちたときは入力を直して丸ごと再送する
- 出力は入力順の `KEY-N<TAB>URL<TAB>title`（URL が無いときは空の列）。`--json` は API の応答の形（`tasks` 配列）で、
  各要素に `tasks create` と同じ `key` と `url` が付く
- 作成は件数分の通知・`task.created` Webhook を出す（`tasks create` を件数分叩いたのと同じ）

- 期限は RFC 3339（`2026-09-30T12:00:00Z`）か日付だけ（`2026-09-30` = その日の始まり `00:00:00Z`）。
  日付だけのときに何時になるかは Web UI と揃えている
- ソフト期限はハード期限より**前**でなければならない。同時刻は API が 400 を返す
- `--label` は今のラベルを**置き換える**。残したまま増減するなら `--add-label` / `--remove-label`
- `--type` は種別を 1 つに決める（付け替えもこれ）。外すときは `--clear-type`。種別の定義はプロジェクトごとに違う
- `--assignee` も今の担当者を**置き換える**。更新では足りない人を足し、外れた人を外す
  （既にいる人の役割は変えない）。全員外すときは `--clear-assignees`。作成・更新とも、
  同じユーザー ID に解決される指定は一人分にまとめる
- 渡さなかった項目は送らないので、既存の値は消えない。消すときは `--clear-*` を使う
- `--description` と `--description-file` が空（空白だけを含む）なら、API を呼ばず終了コード 2 で止まる。
  途中のコマンドが失敗して空のファイルができても、本文は消えない。本文を消すのは `--clear-description` だけ。
  `tasks create` も同じ（本文なしで作るなら、どちらも付けない）

### PR との連携

タスクに結ばれた PR を一覧し、PR 番号で手動リンク・解除する
（仕様は `docs/dev/features/tasks/github-tasks/index.md` の「API」）。

```bash
task tasks pr list TASK-181                 # 連携リポジトリと、リンク済み PR（番号・状態・closes・リンク元・タイトル）
task tasks pr link TASK-181 87 --closes     # --closes でマージ時にタスクを完了にする
task tasks pr unlink TASK-181 87
```

- リンク先のリポジトリはプロジェクトの現在の GitHub 連携先で決まる。連携していなければ
  `github-integration-missing` を含むエラーで終了する（終了コード 1）
- 解除はリンク元（タイトル・本文・ブランチ・手動）によらずすべて外す。タイトル・本文に
  `KEY-N` が残っていれば、次の PR イベントで自動リンクが戻る
- 解除する PR は一覧から番号で引く。リンクされていない番号は終了コード 5。別リポジトリの
  同じ番号もリンクされていれば現在の連携先のものを選び、選べなければ候補を並べて終了コード 2

## 設定

`~/.config/task/config.yaml`（トークンを含むので `0600` で保存する）。環境変数が優先される。

| 設定 | 環境変数 | 環境変数の効き方 |
|---|---|---|
| `api_url` | `TASK_API_URL` | `TASK_TOKEN` と組で渡す |
| `token` | `TASK_TOKEN` | `TASK_API_URL` と組で渡す |
| `tenant_id` | `TASK_TENANT` | 単独で上書きできる。UUID か名前（slug） |

```bash
task config set api_url https://task.example.com/api
task auth token < token.txt        # 引数を省くと標準入力から読む
task auth whoami                   # 使用中の鍵と持ち主・scope・有効期限・使えるテナント（tenant_ids）が出る
task tenants list                  # 使えるテナントの ID・名前（slug）・表示名（read:tenant が要る）
```

`task config set api_url` は、保存の前に API が答えるかを確かめる。送るのは認証なしの要求 1 種類
（`GET <URL>/v1/personal_tokens/me`）で、トークンは送らない。`https://task.example.com` と書けば
`/api` を足した形も試し、答えた方を保存して、保存した値を出力に出す。どちらも答えなければ終了コード 2 で
止まり、config は変えない。届かない環境で先に書いておくなら `--no-verify` を付ける（値をそのまま保存し、何も送らない）。
`api_url` 以外のキーは確かめない。

### 初期設定（task init）

`task init` は、接続の設定とリポジトリの `.koyori-task.yaml` をまとめて用意する。

```bash
task init                          # 端末で、足りないものだけを聞く
task init --api-url https://task.example.com --project TASK < token.txt   # 端末でないとき
```

- 接続（`api_url` と `token`）が未設定なら、インスタンスの URL を聞いて `config set api_url` と同じ方法で確かめ、
  確かめた URL にだけトークンを送る。`GET /v1/personal_tokens/me` が通ってから、両方を 1 回で保存する。
  通らなければ何も保存しない。片方だけ設定済みなら足りない方だけを足し、既にある値は変えない
- トークンの入力は画面に出ない
- 書く先はトップレベルで、`--profile work` を付けるとそのプロファイル（無ければ作る）。
  `TASK_PROFILE` で選んだプロファイルの設定が足りないときと、環境変数を片方だけ渡した状態では、聞かずに終了コード 2 で止まる。
  `TASK_API_URL` と `TASK_TOKEN` を両方渡したときは、それを接続に使い、config には書かない
- 複数のテナントで使えるトークンは、端末でテナントを選ばせて `tenant_id` を同じ層へ保存する。
  端末でないときと、接続を環境変数か `TASK_PROFILE` から取るときは、選ばせずに終了コード 2 で止まる
- git リポジトリの中なら、`origin` の remote（`github.com` の URL だけ）とプロジェクトの連携先を照合する。
  一致が 1 つならそれを確かめ（`--yes` で省ける）、0 個か複数なら一覧から番号かキーで選ぶ。
  `--project <キーか UUID>` を渡すと照合と選択を飛ばす
- 決まったプロジェクトを、リポジトリの根の `.koyori-task.yaml` に書く。`--profile` を付けたときは `profile` も書く。
  カレントディレクトリから根までのどこかに既に `.koyori-task.yaml` があれば、作らずにその場所と中身を出して終わる
  （壊れたファイルでも上書きしない）。git の外では接続の設定だけを行う
- 特定のプロジェクトに限ったトークンは一覧を引けないので、プロジェクトの UUID を聞く（または `--project <UUID>`）。
  ファイルにも UUID を書く
- 端末でないときは 1 つも聞かない。足りない入力（`--api-url`、標準入力のトークン、`--project` か `--yes`）があれば
  終了コード 2 で止まる。選ばずに終えたとき・API が失敗したときは `.koyori-task.yaml` を作らない
- 問いと案内は標準エラーに出す。`--json` の標準出力は結果の JSON 1 つ

### 設定の優先順位

API の URL とトークンは組で決める。片方ずつ config から補うと、あるインスタンスの
トークンを別のインスタンスへ送ってしまうため。

1. `TASK_API_URL` と `TASK_TOKEN` が両方あれば、両方とも環境変数を使う。config は `tenant_id` も含めて読まない
   （環境変数のトークンは別テナントのことが多いため。config が無くても壊れていても動く）。
   テナントは `TASK_TENANT` があればそれ、無ければトークンから引く。プロファイルの指定は見ない（無い名前でも止まらない）
2. どちらも無ければ、両方とも config を使う。選ばれたプロファイル（[プロファイル](#プロファイル)）があればそこから、無ければトップレベルから取る
3. 片方だけなら、config が揃っていても終了コード 2 で止まり、API には何も送らない

空文字の環境変数（`TASK_TOKEN=` や `TASK_TENANT=`）は未設定として扱う。`config *` / `auth token` / `auth logout`
は接続設定を読まないので、片方だけの状態でも動く。外へ出るのは `config set api_url` の確認だけで、
トークンは送らない（`--no-verify` を付ければ何も送らない）。

以前は項目ごとに環境変数が優先されたので、`TASK_TOKEN` だけを渡して URL は config から
読ませる使い方ができた。今はエラーになるので、次のどちらかに揃える。

```bash
# 両方を環境変数で渡す
export TASK_API_URL=https://task.example.com/api
export TASK_TOKEN=...

# 両方を解除して config に揃える
unset TASK_API_URL TASK_TOKEN
task config set api_url https://task.example.com/api
task auth token < token.txt
```

`tenant_id` は、トークンが使えるテナントが 1 つなら任意。未設定ならそのテナントを
`GET /v1/personal_tokens/me` から引く（config には書かない）。複数のテナントで使えるトークンは
自動では選ばず、`TASK_TENANT` か `tenant_id` の指定を求めて終了コード 2 で止まる。
このとき使えるテナントを ID と名前（slug）で並べる（名前は `read:tenant` があるときだけ。無ければ ID だけ）。

`TASK_TENANT` と `tenant_id` には UUID のほか、名前（slug。`task tenants list` で調べる）も書ける。
名前は完全一致を優先し、無ければ大文字小文字を無視して照合する。無視した照合で複数に合うときは、候補（名前と UUID）を並べて終了コード 2 で止まる。
名前のときは `GET /v1/tenants` を 1 回叩いて ID に直すので、トークンに `read:tenant` が要る
（無ければ終了コード 4 で止まり、UUID を使う案内を出す）。見つからなければ使える名前を並べて終了コード 2 で止まる。
UUID のときは何も叩かない。
設定していればそちらを使うので、使えないテナントのトークンに替えたら
`task config unset tenant_id`（環境変数なら `TASK_TENANT` の解除）で外す。外さないと 403 になる。

トークンは Web のアカウント設定（アクセストークン）で発行する。CLI のすべての操作を使うなら
`api`、読むだけなら `read_api` を付ける。資源ごとに絞るなら `read:task` / `write:review` の
ように並べる（`write:*` は対の `read:*` を含む）。

### プロファイル

別のインスタンスやテナントのトークンは、名前付きのプロファイルとして `profiles` に置ける。

```yaml
api_url: https://task.example.com/api   # トップレベル（プロファイルを選ばないとき）
token: ...
profiles:
  work:
    api_url: https://task.work.example/api
    token: ...
    tenant_id: acme                      # 任意。複数テナントのトークンならここで選ぶ
```

```bash
task config set --profile work api_url https://task.work.example/api   # 無ければ作る
task auth token --profile work < work-token.txt
task --profile work tasks list --project APP
```

- 選ぶ順は `--profile` > `TASK_PROFILE` > `.koyori-task.yaml` の `profile` > トップレベル。上位があれば下位の名前は確かめない。空文字の `TASK_PROFILE` は未設定として扱う
- `default` はトップレベルを指す予約名。`--profile default` で、`TASK_PROFILE` や `.koyori-task.yaml` の指定を外してトップレベルを使う。`profiles.default` は作れない
- プロファイルは丸ごと選ぶ。`api_url` / `token` / `tenant_id` をトップレベルから補わない。選んだ名前が無い、または `api_url` か `token` が欠けていれば、終了コード 2 で止まり、API には何も送らない
- テナントは `TASK_TENANT` > プロファイルの `tenant_id` > トークンから引く、の順。トップレベルの `tenant_id` は使わない
- `TASK_API_URL` と `TASK_TOKEN` が両方あれば、プロファイルの指定は見ない（[設定の優先順位](#設定の優先順位)）
- `config set|get|unset|list` / `auth token` / `auth logout` の対象は `--profile` かトップレベルだけで、`TASK_PROFILE` と `.koyori-task.yaml` は見ない。`TASK_PROFILE=work` のまま引数なしで `task auth logout` すると、トップレベルのトークンが消える。対象は出力の末尾に `(profile: work)` / `(profile: default)` で出す
- プロファイルを作れるのは `config set --profile` だけ。ほかの書く操作は、無い名前で終了コード 2 になり、ファイルを書かない。値を全部外してもプロファイルは `work: {}` として残る
- `config list` はトップレベルの値と、プロファイルがあれば名前の一覧（`profiles`）を出す。`config list --profile work` はそのプロファイルの値を出す
- 名前に使えるのは英数字・`_`・`-` の 1〜64 文字
- プロファイルを知らない古い版の CLI で `config set` などを実行すると、`profiles` が消える

### リポジトリの既定のプロジェクト（.koyori-task.yaml）

リポジトリの根に `.koyori-task.yaml` を置くと、`--project` を省いたときにその `project` を使う。
`task init` で作れる（[初期設定](#初期設定task-init)）。

```yaml
project: TASK   # キーか UUID
```

- 探すのはカレントディレクトリから上へ、`.git`（ディレクトリでもファイルでも）のある階層まで。それより上は見ない。`.git` が見つからなければ読まない
- 採用するのは最も近い 1 ファイルだけで、上の階層のファイルとは合成しない
- プロジェクトは `--project` > `.koyori-task.yaml` の `project` の順で決める。どちらも無ければ終了コード 2 で止まり、API には何も送らない
- `KEY-N` の参照には使わない（参照内のキーを使う）。タスク UUID の参照には使う
- ファイルの値を使ったときは、`project: TASK (from /path/.koyori-task.yaml)` を標準エラーに 1 行出す。`--json` の標準出力は変えない
- `profile: work` と書くと、接続にそのプロファイルを使う（`--profile` と `TASK_PROFILE` が優先。[プロファイル](#プロファイル)）。使ったときは `profile: work (from /path/.koyori-task.yaml)` を標準エラーに 1 行出す
- 知らないキーは標準エラーに警告を出して無視する
- 壊れたファイル（YAML として読めない・空の値）は、`--project` を明示していても API を叩くコマンドを終了コード 2 で止める。`config *` / `auth token` / `auth logout` / `self-update` はファイルを読まないので、壊れていても動く

特定のプロジェクトに限ったトークンはプロジェクトの一覧を引けないので、キーではなくプロジェクトの UUID を
`--project` か `project` に書き、タスクも UUID で指す（トークンに `read:project` も要る）。tenant を
トークンから引いたときは、キーを引く前に終了コード 4 で止まってこの旨を案内する。tenant を設定しているときは
一覧の 403（終了コード 4）がそのまま返る。

## 指定できる値の調べ方

名前で指定する引数は、綴りを外したときにエラーへ候補が出る。事前に知りたいときは次で引ける。

```bash
task projects list                        # --project に渡せるキー
task projects statuses --project TASK     # --status に渡せる名前（プロジェクトごとに違う）
task projects labels --project TASK       # --label に渡せる名前（色と説明も並ぶ）
task projects types --project TASK        # --type に渡せる名前（並び順。色も並ぶ）
```

固定の値は `--help` に列挙してある（`--priority`、`sprints list --status`、`review --state` / `--severity`、`my list --filter`）。

## 終了コード

ゲートとして使えるよう、失敗の理由を終了コードで分ける。

| コード | 意味 |
|---|---|
| 0 | 成功 |
| 1 | ゲート不成立（`review summary`）、またはその他の失敗 |
| 2 | 引数・投入 JSON の検証エラー、設定不足 |
| 3 | 認証失敗（401） |
| 4 | 権限不足（403） |
| 5 | 対象が見つからない（404） |

## API の型

リクエスト / レスポンスの型は backend の `payload` クレートをそのまま使う。手書きの
型定義を挟まないので、API 表面が変わったのに CLI が追従していない状態はコンパイル
エラーになる（#647 で手書き `paths.ts` が実際の応答とずれていた）。

多くのコマンドの前段で読む応答（`/personal_tokens/me` のテナントなど）は、使う項目だけの型を
`payload` に置いて読む。使わない項目が増減しても、その応答を使うコマンドだけが失敗し、ほかは止まらない。
元の応答を直列化したものが読めることは `payload` のテストで固定している。

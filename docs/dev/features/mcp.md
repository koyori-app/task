---
title: MCP サーバー
description: MCP クライアントが PAT か OAuth でタスクを読み書きできるエンドポイント
status: partial
updated: 2026-11-21
sidebar:
  order: 7
---

## 概要

backend は MCP（Model Context Protocol）のサーバーを `POST /mcp` で公開する。
Claude Code などの MCP クライアントは、URL と PAT を設定するか、OAuth で接続を許可すると、タスクを読み書きできる。
CLI を入れにくい環境で、CLI の代わりに使うためのものである。
OAuth の認可サーバーも backend が兼ね、claude.ai、Claude Code、ChatGPT、Codex からコネクタとして接続できる。
ツールはタスクを実装する流れで使う 12 個と、レビュー指摘を扱う 6 個である。

## 用語

| 語 | 意味 |
|---|---|
| 許可 | 利用者が OAuth の同意画面で 1 つのクライアントに与えた接続。テナントと scope を持ち、アクセストークンとリフレッシュトークンの元になる |
| クライアント | OAuth で接続する MCP クライアント。`client_id` は、クライアントのメタデータの文書を置いた https の URL である（Client ID Metadata Document。以下 CIMD） |
| 公開の origin | 設定 `allow_origin` の値。画面の origin であり、OAuth の issuer でもある |

## 振る舞い

### エンドポイント

公開 URL は `<公開の origin>/api/mcp` である。backend から見たパスは `/mcp` で、frontend のサーバーが `/api/mcp` を backend の `/mcp` へ転送する。
frontend のサーバーが backend へ転送するのは、`/api/v1/` で始まるパス、`/api/mcp`、OAuth の 4 つのパスだけである。
OAuth の 4 つのパスは [API](#api) の節の表にある。

通信は Streamable HTTP で、サーバーはセッションを持たない。
1 つの要求に、`Content-Type: application/json` の応答を 1 つ返す。
`GET` と `DELETE` は 405 を返し、サーバーからの通知の配信は行わない。

`Origin` ヘッダーの付いた要求は設定 `allow_origin` と同じ origin のものだけを通し、ほかは 403 を返す。
`Origin` の無い要求は通す。

### 対応する MCP のバージョン

SDK の rmcp 3.11 が対応するバージョンの要求を受ける。
`initialize` から始める `2025-11-25` の形と、`initialize` を使わない `2026-07-28` の形を試験で固定している。
`initialize` の応答の `serverInfo.name` は `task` である。

### 認証

認証は `Authorization: Bearer` だけを受ける。値は PAT か、OAuth のアクセストークンである。

| 要求 | 応答 |
|---|---|
| `Authorization` が無い。セッション Cookie だけの要求を含む | 401 `unauthorized` |
| 無効な PAT、失効や期限切れの PAT | 401 `unauthorized` |
| 無効なアクセストークン、期限切れや取り消した許可のアクセストークン、リフレッシュトークン | 401 `unauthorized` |
| Device Token | 403 `forbidden` |
| 停止されたアカウントの PAT かアクセストークン | 403 `account-suspended` |

401 の応答には `WWW-Authenticate` が付く。値は次のとおりである。

| 要求 | `WWW-Authenticate` |
|---|---|
| `Authorization` が無い | `Bearer resource_metadata="<公開の origin>/.well-known/oauth-protected-resource/api/mcp", scope="read:tenant read:project read:task write:task read:milestone read:sprint read:review write:review"` |
| Bearer はあるが無効 | 上の値の `Bearer` の直後に `error="invalid_token", ` を足したもの |

### OAuth での接続

接続は次の順に進む。

1. クライアントが `/mcp` の 401 の `resource_metadata` から Protected Resource Metadata を読み、認可サーバーの issuer を知る
2. クライアントが Authorization Server Metadata を読み、認可、トークン、失効の各エンドポイントを知る
3. クライアントが利用者のブラウザーを同意画面の `/oauth/authorize` へ送る。要求には `client_id`、`redirect_uri`、PKCE の `code_challenge`（`S256`）、`state` が付く
4. 利用者がテナントと scope を選んで許可すると、画面がクライアントのコールバックへ戻る。戻り先には `code`、`state`、`iss` が付く
5. クライアントがトークンのエンドポイントで `code` と `code_verifier` をトークンの組に替える
6. クライアントがアクセストークンを `Authorization: Bearer` に付けて `/mcp` を呼ぶ

クライアントの登録は CIMD だけで行う。
backend は `client_id` の URL からメタデータの文書を取り、`client_id` が URL と一致すること、`redirect_uris`、認証方式を確かめる。
受ける認証方式は `none` だけで、クライアントの認証を付けたトークンの要求は 401 `invalid_client` で断る。
文書は 5 KB までを読み、リダイレクトは追わず、特殊用途のアドレスへ解決する URL は取りに行かない。
取得した文書は、応答の `Cache-Control` の `max-age` を 60 秒から 86400 秒の間に丸めた時間だけ保存する。`max-age` が無ければ 300 秒である。`no-store` か `no-cache` の付いた文書は、下限の 60 秒だけ保存する。
保存していない文書を取りに行けるのは、利用者ごとに 60 秒あたり 10 回までである。

`redirect_uri` は、文書の `redirect_uris` のどれかと文字列として完全に一致しなければならない。
`http` の loopback（`localhost`、`127.0.0.1`、`[::1]`）だけは、ポートの違いを無視する。

認可コードの寿命は 300 秒で、1 回だけ使える。
同じコードがもう一度来たら、そのコードから作った許可を取り消す。

アクセストークンの寿命は 1 時間、リフレッシュトークンの寿命は 90 日である。
リフレッシュトークンは使うたびに新しいものに替わり、許可の期限も 90 日先に延びる。
替えたあとの古いリフレッシュトークンがもう一度来たら、許可を取り消す。
クライアントの文書が `refresh_token` の grant を宣言していなければ、リフレッシュトークンは発行せず、許可は 1 時間で切れる。

アクセストークンは `/mcp` 宛てに発行する。REST API（`/api/v1/`）に直接付けた要求は 401 `unauthorized` になる。
トークンと認可の要求の `resource` は、`<公開の origin>/api/mcp` を指すものだけを受ける。`resource` の無い要求は通す。

許可は、次のどれでも使えなくなる。

- 利用者が「連携中のアプリ」で接続を解除する
- クライアントが失効のエンドポイントを呼ぶ
- 利用者がパスワードを再設定する
- 管理者が利用者を停止する
- 許可を結び付けたテナントがすべて、削除されるか、一括失効で外される
- 運用者がクライアントを許可するクライアントの一覧から外す

### 許可するクライアントの設定

接続を受けるクライアントは、backend の環境変数 `OAUTH_ALLOWED_CLIENT_IDS` で決める。値は `client_id` のカンマ区切りで、照合は文字列の完全一致である。

| 値 | 受けるクライアント |
|---|---|
| 未設定、空文字 | 既定の 4 つ |
| `*` | 任意の https の `client_id` |
| `none` | なし。新しい同意も、既存の許可も止まる |
| それ以外 | 書いた `client_id` だけ。既定の 4 つは含まない |

既定の 4 つは次のとおりである。

| クライアント | `client_id` | 戻り先 |
|---|---|---|
| claude.ai のコネクタ | `https://claude.ai/oauth/mcp-oauth-client-metadata` | https |
| Claude Code | `https://claude.ai/oauth/claude-code-client-metadata` | loopback |
| ChatGPT | `https://chatgpt.com/oauth/client.json` | https |
| Codex CLI | `https://chatgpt.com/oauth/codex/client.json` | loopback |

本番の compose は、`.env` に書いた `OAUTH_ALLOWED_CLIENT_IDS` を backend に渡す。
一覧から外したクライアントの許可は、backend を再起動した時点で使えなくなる。一覧に戻せば、期限の内の許可は再び使える。
一覧に関係なく、メタデータの 2 つのパスは常に応答し、PAT での接続も使える。

### claude.ai での設定

カスタムコネクタを追加し、URL に `<公開の origin>/api/mcp` を入れる。
認証の方式は「Use Claude's published identity」を選ぶ。「Register automatically」は動的なクライアントの登録を使うので、接続に失敗する。

### ChatGPT での設定

コネクタを追加し、URL に `<公開の origin>/api/mcp` を入れる。クライアントの登録の方式は CIMD を選ぶ。

### Codex での設定

次のコマンドで追加し、ログインする。クライアントの登録の方式は既定の `auto` のままでよい。

```bash
codex mcp add task --url https://task.example.com/api/mcp
codex mcp login task
```

### Claude Code での設定

このリポジトリは、直下の `.mcp.json` に本番のエンドポイントを置いている。

```json
{
  "mcpServers": {
    "task": {
      "type": "http",
      "url": "https://task.koyori.app/api/mcp"
    }
  }
}
```

`headers` が無いので、Claude Code は OAuth で接続する。
初めてリポジトリを開くと、このサーバーを使うかを聞かれる。承認したあと `/mcp` で `task` を選ぶと、ブラウザーで同意画面が開く。
`claude -p` は同意画面を出せないので、先に対話のセッションの `/mcp` か `claude mcp login task` でログインしておく。

PAT で接続するときは、`headers` に PAT を書く。次の例は、`headers` を書いた `.mcp.json` を置くリポジトリ向けである。このリポジトリの `.mcp.json` は `headers` を持たないので、`TASK_TOKEN` を読まない。

```json
{
  "mcpServers": {
    "task": {
      "type": "http",
      "url": "https://task.example.com/api/mcp",
      "headers": {
        "Authorization": "Bearer ${TASK_TOKEN}"
      }
    }
  }
}
```

`TASK_TOKEN` は PAT を入れた環境変数である。
Claude Code のクラウド環境では、環境の設定で環境変数 `TASK_TOKEN` を足し、task のホストを Allowed domains に足す。
環境変数はその環境を使う全員が読めるので、PAT は scope とプロジェクトを絞って発行する。

`headers` に `${TASK_TOKEN}` を書いた定義では、`TASK_TOKEN` が未設定だと、Claude Code は設定の警告を出し、接続は失敗する。OAuth には切り替わらない。
このリポジトリで PAT を使うときは、同じ名前の定義を local スコープに足す。`.mcp.json` の定義は user スコープの同じ名前の定義より優先され、local スコープの定義は `.mcp.json` の定義より優先される。user スコープに PAT で `task` を登録していても、このリポジトリでは `.mcp.json` の定義が使われる。コマンドは次のとおりである。

```bash
claude mcp add --transport http --scope local task https://task.koyori.app/api/mcp --header "Authorization: Bearer <PAT>"
```

PAT は `~/.claude.json` に平文で保存され、シェルの履歴にも残る。scope とプロジェクトを絞った PAT を使う。

### ツールの実行

ツールは REST API を backend の中で呼び直し、API の応答の JSON をそのまま文字列で返す。
呼び直すときに元の要求から引き継ぐヘッダーは `Authorization` だけで、Cookie は渡さない。
そのため、ツールでできる操作は PAT か許可の scope、テナントの束縛、プロジェクトの束縛で決まり、PAT で REST API を直接呼んだときと同じになる。

失敗は `isError: true` の結果として返す。
文面は API の状態コードと `message` を並べたもので、例は `403 forbidden`、`404 not-found`、`409 github-integration-missing` である。
引数の誤りは API を呼ぶ前に `parent_task_id must be a UUID` や `findings[1].line: range min: 1` のような文面で返す。

任意の引数のうち `tenant`、`project`、`status`、`priority`、`q`、`sort`、`task_type`、`milestone`、`sprint`、`file`、`repo`、`from_repo`、ID、日付は、空文字を省略として読む。
`title`、`description`、`body`、`summary`、`note` は空文字のまま API へ渡す。

### テナントの決め方

`whoami` を除く全ツールは引数 `tenant` を受ける。

| `tenant` | テナント |
|---|---|
| UUID | そのテナント |
| UUID でない文字列 | `display_id` が一致するテナント。完全一致を優先し、無ければ大文字と小文字を区別せずに 1 件だけ合うもの |
| 省略 | PAT か許可を束縛したテナントが 1 件ならそのテナント |

PAT か許可を複数のテナントに束縛していて `tenant` を省くと、候補の ID を並べたエラーを返す。
サーバーが 1 つを選ぶことはない。

### プロジェクト、タスク、状態の指し方

`project` はプロジェクトのキーか UUID で指す。キーは大文字と小文字を区別しない。

`task` は `KEY-N` の形か UUID で指す。
`KEY-N` のときはキーからプロジェクトを決め、`project` は読まない。
`KEY-N` のキーも大文字と小文字を区別しない。
UUID のときは `project` が必須で、無ければ `project is required when task is a UUID` を返す。

`status` は状態の名前か UUID で指す。
名前は完全一致を優先し、無ければ大文字と小文字を区別せずに探す。
見つからないときは、そのプロジェクトの状態の名前を並べたエラーを返す。
`create_task` で `status` を省くと、プロジェクトの既定の状態を使う。

ラベル、担当者、種別、マイルストーン、スプリントは名前か UUID で指す。
担当者はユーザー名で指す。
名前の照合は `status` と同じで、完全一致を優先し、無ければ大文字と小文字を区別せずに探す。
見つからないときは、そのプロジェクトの候補の名前を並べたエラーを返す。
複数に当たるときは、候補の名前と ID を並べて UUID での指定を求める。
UUID は確かめずに API へ渡す。
担当者の候補の一覧は `write:task` が無いと読めないので、そのときは候補なしと返す。

## データ

許可は `personal_tokens` の行に載せる。PAT の行と同じ列を使い、次の列で見分ける。

| 列 | 型 | 制約 | 説明 |
|---|---|---|---|
| `personal_tokens.oauth_client_id` | VARCHAR | NULL 可 | 許可を与えたクライアントの `client_id`。PAT の行は NULL |

許可の行は、`name` にクライアントの名前、`scopes` に許可した scope、`expires_at` に許可の期限を持つ。
結び付けたテナントは、PAT と同じ `personal_token_tenants` に持つ。

アクセストークンとリフレッシュトークンは `oauth_tokens` に置く。平文は保存しない。

| 列 | 型 | 制約 | 説明 |
|---|---|---|---|
| `id` | UUID | 主キー | |
| `grant_id` | UUID | `personal_tokens.id` を参照、削除は CASCADE | 元になる許可 |
| `kind` | VARCHAR | NOT NULL | `access` か `refresh` |
| `token_hash` | VARCHAR | NOT NULL、UNIQUE | トークンの HMAC-SHA256 |
| `expires_at` | TIMESTAMPTZ | NOT NULL | 期限 |
| `consumed_at` | TIMESTAMPTZ | NULL 可 | リフレッシュトークンを新しいものに替えた時刻 |
| `created_at` | TIMESTAMPTZ | NOT NULL | |

列の定義は [m20261119000000_oauth_grants.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261119000000_oauth_grants.rs) にある。

認可コードと、取得したクライアントの文書は Redis に置く。

## API

`/mcp` と OAuth の公開のエンドポイントは次のとおりである。どれも `openapi.json` に載らない。

| メソッド | 公開のパス | backend のパス | 説明 |
|---|---|---|---|
| POST | `/api/mcp` | `/mcp` | MCP の JSON-RPC 2.0 の要求を 1 件受ける |
| GET | `/.well-known/oauth-protected-resource/api/mcp` | 同じ | Protected Resource Metadata |
| GET | `/.well-known/oauth-authorization-server` | 同じ | Authorization Server Metadata |
| POST | `/api/oauth/token` | `/oauth/token` | コードかリフレッシュトークンを、トークンの組に替える |
| POST | `/api/oauth/revoke` | `/oauth/revoke` | トークンから、その許可を取り消す |

トークンと失効のエンドポイントは、本文を `application/x-www-form-urlencoded` で受ける。
エラーは `{"error": "…", "error_description": "…"}` の形で返し、`Cache-Control: no-store` を付ける。

| トークンのエンドポイントの条件 | 状態 | `error` |
|---|---|---|
| `grant_type` が `authorization_code` と `refresh_token` のどちらでもない | 400 | `unsupported_grant_type` |
| 必須の項目が無い、本文が form でない | 400 | `invalid_request` |
| `resource` がこのサーバーの `/api/mcp` を指さない | 400 | `invalid_target` |
| コードかリフレッシュトークンが無効、期限切れ、使用済み。`client_id`、`redirect_uri`、PKCE のどれかが合わない。許可が使えない | 400 | `invalid_grant` |
| クライアントの認証（`Authorization` ヘッダー、`client_secret`、`client_assertion`）が付いている | 401 | `invalid_client` |

成功の応答は `access_token`、`token_type`、`expires_in`、`scope` を持つ。`refresh_token` は、発行したときだけ持つ。
リフレッシュの要求の `scope` は読まない。応答の `scope` は常に許可の scope である。

失効のエンドポイントは、未知のトークンにも 200 を返す。`client_id` を付けた要求は、許可のクライアントと一致するときだけ取り消す。

同意画面と「連携中のアプリ」が使う API は次のとおりである。形は `openapi.json` にある。

| メソッド | パス | 説明 |
|---|---|---|
| POST | `/v1/oauth_grants/preview` | 認可の要求を確かめ、同意画面に出す中身を返す |
| POST | `/v1/oauth_grants/authorize` | 要求を承認し、コード付きの戻り先を返す |
| POST | `/v1/oauth_grants/deny` | 要求を拒否し、`error=access_denied` 付きの戻り先を返す |
| GET | `/v1/oauth_grants` | 自分の使える許可の一覧 |
| DELETE | `/v1/oauth_grants/{id}` | 許可を取り消す |

認可の要求が受けられないとき、この 3 つの POST は 400 を返し、`message` に理由を入れる。

| `message` | 条件 |
|---|---|
| `oauth-client-not-allowed` | `client_id` が許可するクライアントの一覧に無い |
| `oauth-invalid-client` | `client_id` の URL か、メタデータの文書の中身が受けられない |
| `oauth-client-metadata-unavailable` | 文書を取れない。取得の回数の上限を超えた場合を含む |
| `oauth-invalid-redirect-uri` | `redirect_uri` が文書の登録と合わない |
| `oauth-unsupported-response-type` | `response_type` が `code` でない |
| `oauth-invalid-code-challenge` | `code_challenge_method` が `S256` でない、`code_challenge` の形が違う |
| `oauth-invalid-target` | `resource` がこのサーバーの `/api/mcp` を指さない |
| `oauth-invalid-scope` | `scope` に、渡せる scope が 1 つも無い |

ツールは次のとおりである。

| ツール | 主な引数 | 呼び直す API |
|---|---|---|
| `whoami` | なし | `GET /v1/personal_tokens/me` |
| `list_projects` | なし | `GET /v1/tenants/{tenant}/projects` |
| `list_project_options` | `project`、`kind` | `GET …/projects/{project}/{kind の集合}` |
| `list_tasks` | `project`、`q`、`is_archived`、`sort`、`limit`、`offset` | `GET …/projects/{project}/tasks` |
| `search_tasks` | `project`、`q`、`limit`、`offset` | `GET …/tasks/search` |
| `get_task` | `task`、`project` | `GET …/tasks/{task}` |
| `create_task` | `project`、`title`、`description`、`status`、`priority`、`parent_task_id`、`labels`、`assignees`、`task_type`、`milestone`、`sprint`、`soft_deadline`、`hard_deadline`、`planned_start_on`、`estimated_minutes` | `POST …/projects/{project}/tasks` |
| `update_task` | `task`、`project`、`title`、`description`、`clear_description`、`status`、`priority`、`progress_pct`、`add_labels`、`remove_labels`、`is_archived` | `PUT …/tasks/{task}` |
| `list_task_comments` | `task`、`project` | `GET …/tasks/{task}/comments` |
| `add_task_comment` | `task`、`project`、`body`、`parent_comment_id` | `POST …/tasks/{task}/comments` |
| `list_task_pull_requests` | `task`、`project` | `GET …/tasks/{task}/forge` |
| `link_pull_request` | `task`、`project`、`number`、`closes` | `POST …/tasks/{task}/forge/pull-requests` |
| `list_review_findings` | `project`、`pr`、`repo`、`state`、`severity` | `GET …/projects/{project}/review-findings` |
| `list_review_rounds` | `project`、`pr`、`repo` | `GET …/projects/{project}/reviews` |
| `get_review_summary` | `project`、`pr`、`repo` | `GET …/reviews/summary` |
| `submit_review` | `project`、`pr`、`head_sha`、`summary`、`findings` | `POST …/projects/{project}/reviews` |
| `update_review_finding` | `project`、`finding_id`、`state`、`note` | `PATCH …/review-findings/{finding_id}` |
| `carry_over_review` | `project`、`from_pr`、`to_pr`、`from_repo` | `POST …/reviews/carry-over` |

`create_task` と `update_task` は、`_id` の付いた旧名（`label_ids`、`assignee_ids`、`task_type_id`、`milestone_id`、`sprint_id`、`add_label_ids`、`remove_label_ids`）も同じ引数として受ける。

`list_project_options` の `kind` は `statuses`、`labels`、`task_types`、`milestones`、`sprints`、`assignable_users` のいずれかである。

`list_tasks` の `q` は一覧の条件式で、`search_tasks` の `q` は検索する文字列である。
省いた引数は API へ送らず、API の既定の値が使われる。

`update_task` は渡した項目だけを変える。
`soft_deadline` と `hard_deadline` は RFC 3339 か `YYYY-MM-DD` で渡し、`YYYY-MM-DD` はその日の始まりの UTC として読む。
`priority` の値は API が検証する。

レビューのツールは PR を番号の `pr` で指す。規則は[レビュー指摘](/dev/features/review-findings)にある。
`list_review_findings` の `state` と `severity` は配列で渡す。
`submit_review` の `findings` の各要素は `severity`、`title`、`body`、`file`、`line` を持つ。
`submit_review` は API を呼ぶ前に、`head_sha` が 40 桁の小文字 16 進であること、`title` と `body` が空白だけでないこと、起票の上限と範囲を確かめる。
`get_review_summary` は API の集計をそのまま返す。PR の HEAD との照合は、呼び出し側が `latest_head_sha` で行う。

OAuth で接続したとき、`whoami` は許可の ID と、クライアントの名前を返す。

## 認可

利用できるのは PAT か許可の持ち主だけで、範囲は束縛と scope で決まる。
各ツールに要る scope は次のとおりである。

| 操作 | 誰が | スコープ |
|---|---|---|
| `whoami` | PAT か許可の持ち主 | なし |
| `list_projects` | PAT か許可の持ち主 | `read:project` |
| `list_tasks`、`search_tasks`、`get_task`、`list_task_comments`、`list_task_pull_requests` | PAT か許可の持ち主 | `read:task` |
| `create_task`、`update_task`、`add_task_comment`、`link_pull_request` | PAT か許可の持ち主 | `write:task` |
| `list_project_options` の `statuses`、`labels`、`task_types` | PAT か許可の持ち主 | `read:task` |
| `list_project_options` の `milestones` | PAT か許可の持ち主 | `read:milestone` |
| `list_project_options` の `sprints` | PAT か許可の持ち主 | `read:sprint` |
| `list_project_options` の `assignable_users` | PAT か許可の持ち主 | `write:task` |
| `list_review_findings`、`list_review_rounds`、`get_review_summary` | PAT か許可の持ち主 | `read:review` |
| `submit_review`、`update_review_finding`、`carry_over_review` | PAT か許可の持ち主 | `write:review` |

引数を名前で渡すと、ID を引くための scope が追加で要る。

| 引数 | 追加で要るスコープ |
|---|---|
| `project` がキー、または `task` が `KEY-N` | `read:project` |
| `tenant` が名前 | `read:tenant` |
| `status` が名前、または `create_task` で `status` を省く | `read:task` |
| ラベルか種別が名前 | `read:task` |
| マイルストーンが名前 | `read:milestone` |
| スプリントが名前 | `read:sprint` |

担当者をユーザー名で渡すときは、追加の scope は要らない。

ID を引く呼び出しが 403 のときは、考えられる原因を文面に添える。
403 は scope が足りないときのほか、束縛していないテナントやプロジェクトを指したときにも返るので、原因は断定しない。
例は `403 forbidden (could not look up project key "TASK": the token may lack the read:project scope, or it is not allowed in this tenant; if the scope is missing, pass the project UUID instead)` である。

OAuth の同意と許可の管理は次のとおりである。

| 操作 | 誰が | スコープ |
|---|---|---|
| 同意画面での許可（preview、承認、拒否） | ログイン中の本人。選んだすべてのテナントに所属していること | セッション専用 |
| 許可の一覧、解除 | ログイン中の本人 | セッション専用 |

同意の条件は PAT の発行と同じで、[個人アクセストークンの認可](/dev/design/personal-access-tokens-authz#発行の認可誰が作れるか)にある。
所属は、オーナーか、テナントのメンバー（Admin、Member、Viewer）であることを指す。プロジェクトだけに参加する利用者は同意できない。
テナントが 2FA を必須にしていれば、2FA を設定した利用者だけが同意できる。
選んだテナントのうち 1 つでも条件を満たさなければ、全体を断る。

OAuth で渡せる scope は、ツールが使う `read:tenant`、`read:project`、`read:task`、`write:task`、`read:milestone`、`read:sprint`、`read:review`、`write:review` の 8 つに限る。
利用者は同意画面で、クライアントが求めた scope の一部だけを許可できる。

届く範囲は、要求のたびに、許可の scope、テナントの束縛、本人のその時点の所属とロールで決まる。
ロールを下げても、テナントから外しても、許可は取り消さない。
テナントから外れた利用者の許可では、そのテナントのテナント全体の操作（`list_projects` など）が 403 `tenant-membership-missing` になる。
ただし、プロジェクトのメンバーとして明示した指定が残っていれば、そのプロジェクトを指すツールは通り、`list_projects` はそのプロジェクトだけを返す。
テナントに戻れば、許可は元どおりに使える。

## 画面

| 項目 | 値 |
|---|---|
| URL | `/oauth/authorize` |
| 入口 | MCP クライアントが、認可の要求を query に付けて利用者のブラウザーを送る |

同意画面はログイン前でも開ける。未ログインならサインインへ送り、サインインのあとで同じ要求に戻る。
要求の query はブラウザーの `sessionStorage` に置き、URL を `/oauth/authorize?resume=<キー>` に替える。別のタブで開き直した場合や、許可のあとで戻った場合は、クライアントからやり直すよう案内する。

画面に出すものは次のとおりである。

- `client_id` の host。クライアントが名乗る名前は自己申告なので、host を主に出し、名前は添えて出す
- 許可のあとの戻り先の host
- テナントの選択。候補は自分が所属するテナントで、1 件なら選んだ状態で始める
- クライアントが求めた scope のチェックボックス。すべてチェックした状態で始める
- 「許可する」と「拒否する」

戻り先が利用者の端末（loopback）のクライアントでは、表示を次のように変える。対象は Claude Code と Codex CLI である。

- 「この端末で動くアプリが接続を求めています。自分で Claude Code や Codex から接続を始めた直後でなければ、拒否してください。」を出す
- 戻り先をポートまで出す
- テナントは、候補が 1 件でも選んでいない状態で始める
- `write:task` と `write:review` はチェックを外した状態で始める

テナントを 1 つ以上選ぶまで「許可する」は押せない。候補のテナントが無ければ「許可する」を出さない。
認可の要求が受けられないときは、理由を出して止まり、クライアントへは戻らない。
ほかのサイトの枠の中では、同意画面を出さない。応答に `Content-Security-Policy: frame-ancestors 'none'` と `X-Frame-Options: DENY` を付け、画面も枠の中かどうかを確かめる。

許可した接続の一覧と解除は、アカウント設定の[連携中のアプリ](/dev/features/account-settings#連携中のアプリ)にある。

## 未実装と既知の制限

- レビューのツールを足す前に与えた許可は `read:review` と `write:review` を持たない。レビューのツールを使うには接続し直す
- `get_review_summary` は HEAD との照合をしない。CLI の `review summary` と違い、呼び出し側が `latest_head_sha` を比べる
- 連携を張る前に記録したラウンドは、レビューのツールでは読めず、引き継ぎ元にもできない。CLI の `--repo ""` と `--from-repo ""` を使う
- タスクの一括作成と削除、PR のリンクの解除、自分のタスク、スプリント、通知、テナントの一覧のツールは未実装である（TASK-561）
- `update_task` で変えられるのは、表に挙げた項目だけである。期限、親タスク、担当者などの変更は REST API か CLI を使う
- Claude Code のクラウド環境の network secrets で PAT を渡せるかは確かめていない
- 動的なクライアントの登録（DCR）は受けない
- `private_key_jwt` でのクライアントの認証は受けない
- scope を後から足す手順（403 `insufficient_scope`）は無い。ツールの scope の不足は、従来どおり `isError` の結果で返る
- 同意画面では、プロジェクト単位の束縛を選べない
- ブラウザーから直接トークンを取る MCP クライアント向けの CORS は無い
- 認可の要求の誤り（`response_type`、PKCE、`resource`、`scope`）は画面に出すだけで、クライアントへはリダイレクトで伝えない
- 同じクライアントで接続し直すと、古い許可が期限まで一覧に残り、同じ名前が並ぶ。作成日時は持たないので、最終使用と期限で見分ける
- パスワードの再設定や一括失効などで取り消した許可の `oauth_tokens` の行は、利用者を削除するまで残る
- リフレッシュトークンの再利用は猶予なく許可を取り消すので、応答が届かなかった再送や、同時の更新でも、接続し直しになる
- Codex の IDE 拡張とクラウドの Codex が、どの `client_id` で接続するかは確かめていない。既定の 4 つと違えば、`OAUTH_ALLOWED_CLIENT_IDS` に足す
- claude.ai、ChatGPT、Codex からの実際の接続は確かめていない
- Claude Code から PAT で接続すること（`.mcp.json` の `headers` と、local スコープの上書きの両方）は確かめていない
- このリポジトリの `.mcp.json` で、Claude Code のクラウド環境から接続できるかは確かめていない

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-11-18 | MCP サーバーを backend の `/mcp` に置く | 別のバイナリや CLI のサブコマンドにすると、配る物か導入の手間が増える。URL と PAT だけで使える形が、CLI を入れにくい環境という目的に合う |
| 2026-11-18 | ツールは REST API を backend の中で呼び直す | scope とテナントの束縛の検査はハンドラーにある。呼び直せば同じ検査を通り、認可の規則が 2 箇所に分かれない |
| 2026-11-18 | 認証は PAT の Bearer だけにし、Cookie と Device Token を断る | PAT は scope、テナント、プロジェクトで渡す範囲を絞れる。Device Token は scope を持たず、束縛したテナントも引けない。OAuth は TASK-563 で扱う |
| 2026-11-18 | 最初のツールを 12 個にする | タスクを読む、状態を変える、コメントする、PR をつなぐ流れを満たす。残りは同じ形で TASK-560 と TASK-561 で足す |
| 2026-11-18 | `.mcp.json` をリポジトリに置かず、設定例を書く | URL は配置先ごとに違う。接続先が決まってから TASK-564 で足す |
| 2026-11-18 | frontend のサーバーの転送に `/api/mcp` の 1 パスだけを足す | backend は frontend のサーバー越しにしか公開されない。`/api` 全体を転送すると、backend の API 仕様の画面などの `v1` の外のパスまで公開される |
| 2026-11-19 | 認証に OAuth のアクセストークンを足す。2026-11-18 の「PAT の Bearer だけ」を改める | claude.ai のコネクタで固定のヘッダーを使えるのは一部の組織だけで、全組織で使うには MCP の認可の仕様に沿った OAuth が要る |
| 2026-11-19 | 転送に OAuth の 4 つのパスを足す。2026-11-18 の「1 パスだけ」を改める | MCP クライアントは公開の origin からメタデータとトークンのエンドポイントを引く。足すのは決まった 4 つのパスだけで、`/api` 全体は転送しない |
| 2026-11-19 | 許可を `personal_tokens` の行に載せる | scope、テナントの束縛、失効、停止の検査が PAT と同じコードを通る。別の表にすると、既存の失効の経路とロックの順を写す必要があり、写し漏れが失効の抜けになる |
| 2026-11-19 | アクセストークンは `/mcp` でだけ通す | トークンは `/api/mcp` 宛てに発行する。MCP の仕様は、サーバーが自分宛てのトークンだけを受けることを求める。REST API でも通すと、漏れたトークンが MCP のツールに無い操作まで届く |
| 2026-11-19 | クライアントの登録は CIMD だけにし、DCR を入れない | MCP の 2026-07-28 の改訂は DCR を非推奨にした。DCR は未認証の書き込みと、増え続ける登録の掃除が要る。対象の 4 つのクライアントは CIMD で接続できる |
| 2026-11-19 | 受けるクライアントを許可の一覧で絞り、既定を 4 つの URL の完全一致にする | 外へ取りに行く宛先が、運用者の決めた URL に限られる。前方一致にすると、同じパスの下に置かれる、中身を確かめていない文書まで受ける |
| 2026-11-19 | 一覧から外したクライアントは、既存の許可も使えなくする | 運用者がクライアント単位で許可を止める API も画面も無い。既存の許可を通すと、外したクライアントのリフレッシュトークンが使う限り延び続ける |
| 2026-11-19 | `OAUTH_ALLOWED_CLIENT_IDS=none` で全クライアントを拒否する | 本番の compose は未設定の変数を空文字で渡すので、空を「全部拒否」とは読めない。急いで全部止める手段を設定 1 行で持つ |
| 2026-11-19 | 認証方式は `none` だけを受け、`private_key_jwt` を実装しない | 4 つのクライアントは `none` で接続できる。`private_key_jwt` は署名の検証と鍵の取得を足すので、要るときに設計を分ける |
| 2026-11-19 | 認可の応答に `iss` を付け、メタデータで対応を示す | ChatGPT と Codex は、issuer の識別に対応するサーバーにだけ固定の `client_id` を使う。対応しなければ接続ごとに違う `client_id` になり、許可の一覧で止まる |
| 2026-11-19 | 同意できる人を PAT の発行と同じにする | 許可は本人として動き、届く範囲は要求のたびに本人のロールで決まる。発行と別の規則にすると、PAT に相当するトークンを得る別の経路になる |
| 2026-11-19 | loopback のクライアントは、同意画面の表示を強めるだけにし、寿命と再認証は変えない | なりすましが成り立つのは、利用者の端末で攻撃者のプロセスが動いているときで、その相手は保存済みのトークンにも届く。守れるのは、頼んでいない同意画面に利用者が気づけることである |
| 2026-11-19 | root の `/.well-known/oauth-protected-resource` を出さない | RFC 9728 は、そこで返す `resource` が origin と同じであることを求める。`/api/mcp` を返しても使われない |
| 2026-11-19 | トークンのエンドポイントにレート制限を置かない | frontend のサーバーの転送は接続元を伝えず、全利用者が 1 つの枠に入る。コードとトークンは 256 bit の乱数で、総当たりは成り立たない |
| 2026-11-19 | リフレッシュトークンの再利用は、猶予なく許可を取り消す | 公開クライアントのリフレッシュトークンは回転で守る。猶予を置くと、その間は盗まれたトークンも通る |
| 2026-11-19 | 寿命はアクセストークン 1 時間、リフレッシュトークン 90 日で、使うたびに延ばす | アクセストークンは短くする。90 日は Device Token と PAT の既定に合わせた。使い続ける限り接続し直しを求めない |
| 2026-11-19 | 連携中のアプリを、アカウント設定の新しいページ `/settings/apps` に置く | PAT は利用者が発行するもので、許可はクライアントが求めて利用者が与えるものである。一覧を分けると、解除する対象を探しやすい |
| 2026-11-21 | レビュー指摘のツールを 6 個足す | CLI の `review` の起票、一覧、ラウンド、状態遷移、集計、引き継ぎを、CLI を入れにくい環境でも行える。形はほかのツールと同じで、REST API を呼び直す |
| 2026-11-21 | OAuth で渡せる scope に `read:review` と `write:review` を足し、loopback の同意画面では `write:review` もチェックを外して始める | 渡せる scope はツールが使うものと定めている。足さなければ、一覧に並ぶのに OAuth では 1 つも通らないツールが 6 個できる。`write:review` は繰り延べで通常タスクを作れるので、`write:task` と同じ扱いにする |
| 2026-11-21 | `submit_review` は API を呼ぶ前に、CLI と同じ 3 つを確かめ、同じ文面で返す | API のエラーは入力の全体を含み、先頭 2000 字で切れると上限の値が残らない。短縮 SHA のエラーは 40 桁が要ることを示さない。空白だけの `title` は API が通す。上限と範囲は DTO の規則を走らせるだけで、写しは持たない |
| 2026-11-21 | `get_review_summary` は API の集計をそのまま返し、HEAD との照合を足さない | ほかのツールは API の応答を削らず足さずに返す。応答に `gate`、`mergeable`、`latest_head_sha` があり、判断の材料は揃う。サーバーには作業ツリーも終了コードも無い |
| 2026-11-21 | レビューのツールの `repo` と `from_repo` は、空文字を省略として読む | 使わない引数に空文字を入れるクライアントがある。空文字を連携前のラウンドの指定として読むと、未解決の指摘があっても一覧が空になり、エラーにならないので誤りに気づけない |
| 2026-11-21 | ラベル、担当者、種別、マイルストーン、スプリントの名前の解決を MCP の層で行い、選び方と文面を CLI から写す | `handler` から `cli` は参照できない。`tenant`、`project`、`status` の解決もすでに写しており、名前の選別だけを共有すると写しと共有が混ざる。文面のずれは CLI と同じ表のテストで止める |
| 2026-11-21 | 名前を受ける引数から `_id` を外し、旧名を別名として受け続ける | 名前を受ける既存の引数（`tenant`、`project`、`status`）には `_id` が付かない。CLI のフラグも `--label` などである。引数の構造体は知らない引数を捨てるので、旧名を受けないと、旧名で呼んだときにラベルの無いタスクが黙ってできる |
| 2026-11-21 | `.mcp.json` をリポジトリ直下に置き、`headers` を書かず OAuth で接続する。2026-11-18 の「置かず、設定例を書く」を改める | 本番のエンドポイントが `https://task.koyori.app/api/mcp` に決まった。`headers` に `${TASK_TOKEN}` を書くと、未設定の人に設定の警告が出て接続が失敗し、OAuth に切り替わらない。同じ名前の user スコープの定義にも勝つので、OAuth で接続済みの人の接続も失敗に変わる |

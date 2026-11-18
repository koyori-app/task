---
title: MCP サーバー
description: MCP クライアントが PAT だけでタスクを読み書きできるエンドポイント
status: partial
updated: 2026-11-18
sidebar:
  order: 7
---

## 概要

backend は MCP（Model Context Protocol）のサーバーを `POST /mcp` で公開する。
Claude Code などの MCP クライアントは、URL と PAT を設定するだけでタスクを読み書きできる。
CLI を入れにくい環境で、CLI の代わりに使うためのものである。
ツールはタスクを実装する流れで使う 12 個に限り、レビュー指摘の操作と OAuth での接続は範囲外である。

## 振る舞い

### エンドポイント

公開 URL は `<画面の origin>/api/mcp` である。backend から見たパスは `/mcp` で、frontend のサーバーが `/api/mcp` を backend の `/mcp` へ転送する。
frontend のサーバーが backend へ転送するのは `/api/v1/` で始まるパスと `/api/mcp` だけである。

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

認証は PAT の `Authorization: Bearer` だけを受ける。

| 要求 | 応答 |
|---|---|
| `Authorization` が無い。セッション Cookie だけの要求を含む | 401 `unauthorized` |
| 無効な PAT、失効や期限切れの PAT | 401 `unauthorized` |
| Device Token | 403 `forbidden` |
| 停止されたアカウントの PAT | 403 `account-suspended` |

401 の応答には `WWW-Authenticate: Bearer` が付く。

### ツールの実行

ツールは REST API を backend の中で呼び直し、API の応答の JSON をそのまま文字列で返す。
呼び直すときに元の要求から引き継ぐヘッダーは `Authorization` だけで、Cookie は渡さない。
そのため、ツールでできる操作は PAT の scope、テナントの束縛、プロジェクトの束縛で決まり、REST API を直接呼んだときと同じになる。

失敗は `isError: true` の結果として返す。
文面は API の状態コードと `message` を並べたもので、例は `403 forbidden`、`404 not-found`、`409 github-integration-missing` である。
引数の誤りは API を呼ぶ前に `label_ids must be a UUID` のような文面で返す。

任意の引数のうち `tenant`、`project`、`status`、`priority`、`q`、`sort`、ID、日付は、空文字を省略として読む。
`title`、`description`、`body` は空文字のまま API へ渡す。

### テナントの決め方

`whoami` を除く全ツールは引数 `tenant` を受ける。

| `tenant` | テナント |
|---|---|
| UUID | そのテナント |
| UUID でない文字列 | `display_id` が一致するテナント。完全一致を優先し、無ければ大文字と小文字を区別せずに 1 件だけ合うもの |
| 省略 | PAT を束縛したテナントが 1 件ならそのテナント |

PAT を複数のテナントに束縛していて `tenant` を省くと、候補の ID を並べたエラーを返す。
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

ラベル、担当者、種別、マイルストーン、スプリントは ID で渡す。
ID は `list_project_options` で引く。

### Claude Code での設定

リポジトリの `.mcp.json` に次のように書く。

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

## API

| メソッド | パス | 説明 |
|---|---|---|
| POST | `/mcp` | MCP の JSON-RPC 2.0 の要求を 1 件受ける |

このエンドポイントは `openapi.json` に載らない。

ツールは次のとおりである。

| ツール | 主な引数 | 呼び直す API |
|---|---|---|
| `whoami` | なし | `GET /v1/personal_tokens/me` |
| `list_projects` | なし | `GET /v1/tenants/{tenant}/projects` |
| `list_project_options` | `project`、`kind` | `GET …/projects/{project}/{kind の集合}` |
| `list_tasks` | `project`、`q`、`is_archived`、`sort`、`limit`、`offset` | `GET …/projects/{project}/tasks` |
| `search_tasks` | `project`、`q`、`limit`、`offset` | `GET …/tasks/search` |
| `get_task` | `task`、`project` | `GET …/tasks/{task}` |
| `create_task` | `project`、`title`、`description`、`status`、`priority`、`parent_task_id`、`label_ids`、`assignee_ids`、`task_type_id`、`milestone_id`、`sprint_id`、`soft_deadline`、`hard_deadline`、`planned_start_on`、`estimated_minutes` | `POST …/projects/{project}/tasks` |
| `update_task` | `task`、`project`、`title`、`description`、`clear_description`、`status`、`priority`、`progress_pct`、`add_label_ids`、`remove_label_ids`、`is_archived` | `PUT …/tasks/{task}` |
| `list_task_comments` | `task`、`project` | `GET …/tasks/{task}/comments` |
| `add_task_comment` | `task`、`project`、`body`、`parent_comment_id` | `POST …/tasks/{task}/comments` |
| `list_task_pull_requests` | `task`、`project` | `GET …/tasks/{task}/forge` |
| `link_pull_request` | `task`、`project`、`number`、`closes` | `POST …/tasks/{task}/forge/pull-requests` |

`list_project_options` の `kind` は `statuses`、`labels`、`task_types`、`milestones`、`sprints`、`assignable_users` のいずれかである。

`list_tasks` の `q` は一覧の条件式で、`search_tasks` の `q` は検索する文字列である。
省いた引数は API へ送らず、API の既定の値が使われる。

`update_task` は渡した項目だけを変える。
`soft_deadline` と `hard_deadline` は RFC 3339 か `YYYY-MM-DD` で渡し、`YYYY-MM-DD` はその日の始まりの UTC として読む。
`priority` の値は API が検証する。

## 認可

利用できるのは PAT の持ち主だけで、範囲は PAT の束縛と scope で決まる。
各ツールに要る scope は次のとおりである。

| 操作 | 誰が | スコープ |
|---|---|---|
| `whoami` | PAT の持ち主 | なし |
| `list_projects` | PAT の持ち主 | `read:project` |
| `list_tasks`、`search_tasks`、`get_task`、`list_task_comments`、`list_task_pull_requests` | PAT の持ち主 | `read:task` |
| `create_task`、`update_task`、`add_task_comment`、`link_pull_request` | PAT の持ち主 | `write:task` |
| `list_project_options` の `statuses`、`labels`、`task_types` | PAT の持ち主 | `read:task` |
| `list_project_options` の `milestones` | PAT の持ち主 | `read:milestone` |
| `list_project_options` の `sprints` | PAT の持ち主 | `read:sprint` |
| `list_project_options` の `assignable_users` | PAT の持ち主 | `write:task` |

引数を名前で渡すと、ID を引くための scope が追加で要る。

| 引数 | 追加で要るスコープ |
|---|---|
| `project` がキー、または `task` が `KEY-N` | `read:project` |
| `tenant` が名前 | `read:tenant` |
| `status` が名前、または `create_task` で `status` を省く | `read:task` |

ID を引く呼び出しが 403 のときは、考えられる原因を文面に添える。
403 は scope が足りないときのほか、PAT を束縛していないテナントやプロジェクトを指したときにも返るので、原因は断定しない。
例は `403 forbidden (could not look up project key "TASK": the token may lack the read:project scope, or it is not allowed in this tenant; if the scope is missing, pass the project UUID instead)` である。

## 未実装と既知の制限

- レビュー指摘のツールは未実装である（TASK-560）
- タスクの一括作成と削除、PR のリンクの解除、自分のタスク、スプリント、通知、テナントの一覧のツールは未実装である（TASK-561）
- ラベル、担当者、種別、マイルストーン、スプリントを名前で渡すことはできない（TASK-562）
- OAuth での接続は未実装である。claude.ai のコネクタとして使うには OAuth が要る（TASK-563）
- このリポジトリに `.mcp.json` は置いていない（TASK-564）
- `update_task` で変えられるのは、表に挙げた項目だけである。期限、親タスク、担当者などの変更は REST API か CLI を使う
- Claude Code のクラウド環境の network secrets で PAT を渡せるかは確かめていない

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-11-18 | MCP サーバーを backend の `/mcp` に置く | 別のバイナリや CLI のサブコマンドにすると、配る物か導入の手間が増える。URL と PAT だけで使える形が、CLI を入れにくい環境という目的に合う |
| 2026-11-18 | ツールは REST API を backend の中で呼び直す | scope とテナントの束縛の検査はハンドラーにある。呼び直せば同じ検査を通り、認可の規則が 2 箇所に分かれない |
| 2026-11-18 | 認証は PAT の Bearer だけにし、Cookie と Device Token を断る | PAT は scope、テナント、プロジェクトで渡す範囲を絞れる。Device Token は scope を持たず、束縛したテナントも引けない。OAuth は TASK-563 で扱う |
| 2026-11-18 | 最初のツールを 12 個にする | タスクを読む、状態を変える、コメントする、PR をつなぐ流れを満たす。残りは同じ形で TASK-560 と TASK-561 で足す |
| 2026-11-18 | `.mcp.json` をリポジトリに置かず、設定例を書く | URL は配置先ごとに違う。接続先が決まってから TASK-564 で足す |
| 2026-11-18 | frontend のサーバーの転送に `/api/mcp` の 1 パスだけを足す | backend は frontend のサーバー越しにしか公開されない。`/api` 全体を転送すると、backend の API 仕様の画面などの `v1` の外のパスまで公開される |

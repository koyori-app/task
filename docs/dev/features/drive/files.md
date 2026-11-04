---
title: Drive のファイル
description: Drive のファイルのアップロード・一覧・本文の更新と配信、タスクへの添付と本文からの参照
status: partial
updated: 2026-11-04
slug: features/drive/files
sidebar:
  order: 1
---

## 概要

テナントの利用者が Drive にファイルをアップロードし、一覧・更新・削除・ダウンロードする機能である。Markdown の本文に書いた `KEY-N` をタスクに結び付け、タスクへ手動で添付することもできる。アクセスの規則とストレージは[Drive の概要](/features/drive)に書き、判断の理由は [Drive の決定の記録](/features/drive#決定の記録)に集める。フォルダは[フォルダと共有](/features/drive/folders-and-sharing)、容量は[クォータ](/features/drive/quota)、プロジェクトのドキュメント画面は[ドキュメント](/features/drive/documents)に書く。

## 振る舞い

### 一覧

`GET /v1/tenants/{tenant_id}/drive/files` のクエリは次のとおりである。`total` は絞り込み後の件数である。

| クエリ | 型 | 既定 | 内容 |
|---|---|---|---|
| `folder_id` | UUID? | | フォルダで絞る。省略時はルート |
| `mime_type` | string? | | MIME タイプの前方一致。`text/markdown` は `text/markdown` で始まる MIME（`text/markdown; charset=…` など）にも一致し、`text/` のように末尾 `/` なら種別全体に一致する |
| `q` | string? | | ファイル名の部分一致（`ILIKE`、大文字小文字を区別しない）。200 文字を超えると `400` |
| `sort` | string | `created_at_desc` | `created_at_desc` / `name_asc` / `updated_at_desc`。それ以外は `400`。同順位は `id` 昇順 |
| `limit` | u32 | 50 | 取得件数。`1..=200` に補正する。`0` は 1、200 超は 200 になる |
| `offset` | u32 | 0 | オフセット |

`mime_type` と `q` の `%` `_` `\` は字義どおりに扱い、LIKE のワイルドカードにしない。

### アップロード

`POST /v1/tenants/{tenant_id}/drive/files` は `multipart/form-data` を受け取り、作成した `DriveFile` を `201 Created` で返す。

| フィールド | 必須 | 内容 |
|---|---|---|
| `file` | 必須 | ファイルのバイナリ |
| `name` | 任意 | 表示名。省略時は元ファイル名で、どちらも無ければ `400` |
| `folder_id` | 任意 | アップロード先フォルダの UUID |

処理と制限の規則は次のとおりである。

- フィールドの順序は問わない。`file` はストリームのままストレージへ流し、DB への登録は全パートを読み終えてから行う。登録に失敗したらストレージから消す
- `folder_id` が `file` より前に届いた場合だけ、本文を流す前にフォルダの存在と書き込み権限を確かめて早く落とす
- `mime_type` は、ソースコードの拡張子の上書き表（`.ts` など）、申告された `Content-Type`、ファイル名からの推定の順で決める
- 最大ファイルサイズは `UPLOAD_MAX_SIZE_MB`（既定 100 MB）である。ファイル本体に対する上限で、ちょうどのサイズは受理する
- リクエスト本文全体には、multipart の境界とヘッダーの分として 64 KiB を足した上限を掛ける
- MIME タイプは全種類を許可する
- 空ファイル、`file` パートなし、`file` パートが 2 つ以上は `400 Bad Request` を返す

S3 では次の順に処理する。

1. クライアントが `POST /v1/tenants/{tenant_id}/drive/files`（multipart）を送る
2. バックエンドが multipart のストリームを受信する
3. `object_store` で単一 PUT または multipart upload を実行する
4. DB に `drive_files` の行を登録する。`storage_key` のみを保持する
5. 応答に `/v1/drive/files/{id}/content` を `url` として組み立てて返す

### 名前の変更と移動

`PATCH /v1/tenants/{tenant_id}/drive/files/{id}` は `name` で名前を変更し、`folder_id` に UUID を渡すとそのフォルダへ移動する。`folder_id` の省略は配置を維持し、明示的な `null` はドライブ直下へ移動する。移動元・移動先の権限が必要で、移動に合わせて `project_id` も更新する。

### 本文の更新

`PUT /v1/tenants/{tenant_id}/drive/files/{id}/content` は `{ "content": "更新後の本文" }` を受け取り、更新後の `DriveFile` を `200 OK` で返す。

- 対象は `text/*`、`+json` / `+xml`、JSON・JavaScript・YAML などの許可されたテキスト系 MIME である。対象外は `400 Bad Request` を返す
- 空文字列は許可する
- UTF-8 のバイト数がファイルサイズ上限を超える場合、または差し替え後のテナント使用量がクォータを超える場合は `413` を返す
- 新しいストレージキーへ保存してから DB を更新し、成功後に旧キーの削除を試みる

`If-Match` ヘッダーで楽観ロックできる。

- 値は `DriveFile.etag`（ダブルクォート込み）をそのまま渡す
- カンマ区切りの複数指定はどれか 1 つの一致で通り、`*` は常に一致する
- 弱い ETag（`W/"..."`）は `W/` を外して比べる（TASK-336）
- 一致しなければ `412 Precondition Failed`（`precondition-failed`）を返し、本文は変えない
- 比較はファイル行のロック取得後に行うので、同じ ETag を持った並行更新は片方だけが通り、もう片方は `412` になる
- ヘッダーを省略した場合は無条件に上書きする（後勝ち）
- 応答の `etag` は更新後の値である

### ETag

`DriveFile.etag` は全ファイル API の応答に載る本文の ETag である。値はダブルクォート込みで、`ETag` ヘッダーと同じ文字列である。

| 行の状態 | 値 |
|---|---|
| `content_hash` あり | `"sha256:<content_hash>"`（強い ETag）。例は `"sha256:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae"` |
| `content_hash` なし（導入前の行） | `"ts:<updated_at の Unix ミリ秒>-<size>"` |

本文の更新で `updated_at` が変わるので、導入前の行も `If-Match` で楽観ロックできる。本文を一度更新するとハッシュの形式に切り替わる。名前変更・移動では `content_hash` は変わらず、ハッシュ形式の ETag も変わらない。

### 配信

`GET /v1/drive/files/{id}/content` は本文を `200 OK` で返す。`?token={share_token}` を付けると公開リンクのトークンで配信する。誰が取得できるかは[Drive の認可](/features/drive#認可)の節に書く。

- `Content-Type` を `mime_type` から設定する
- `Content-Disposition: attachment` を常に設定し、ブラウザ上で同一オリジンのコンテンツとして実行させない
- API 共通ミドルウェアが `X-Content-Type-Options: nosniff` を全応答に設定し、宣言した `Content-Type` 以外への MIME sniffing を禁止する
- ストレージバックエンドの `get_stream()` でストリーミング配信し、メモリに全展開しない
- `ETag`（`DriveFile.etag` と同じ値）と `Last-Modified`（`updated_at` を RFC 7231 の HTTP-date で秒精度）を付ける
- `If-None-Match` が現在の ETag に一致すれば `304 Not Modified` を本文なしで返し、`304` にも `ETag` を付ける。ストレージは読まない。カンマ区切りの複数指定と `*` も可で、弱い ETag は `W/` を外して比べる
- `If-Modified-Since` は解釈しない

### タスクへの添付

Drive のファイルは既存のタスクへ添付できる。添付は中間の行の作成・削除で、解除しても Drive のファイル本体は削除しない。

- 添付できるのは同じテナントの一般ファイル、または対象タスクと同じプロジェクトのファイルである。別テナント・別プロジェクトのファイルは `403 Forbidden` を返す
- 同じファイルの二重添付は `409 Conflict` を返す
- 添付の解除は、添付を作成した利用者またはテナントオーナーに限る
- 添付には由来 `source`（`manual` / `body`）があり、応答にも載る
- 本文から付いた `body` の行へ `POST` すると `manual` に昇格して `200 OK` を返す
- `body` の行の `DELETE` は `409 Conflict` を返す。本文を編集して参照を外す

規則の全体は[タスク拡張](/features/tasks/extensions)のファイル添付に書く。

### 本文からのタスク参照

`mime_type` が `text/markdown` のファイルは、アップロード（`POST .../drive/files`）と本文の更新（`PUT .../drive/files/{id}/content`）のたびに本文の `KEY-N` を拾い、`task_attachments` へ `source = 'body'` で結び付ける。DB の更新と同じトランザクションで行う。名前変更・移動では本文を読み直さない。

抽出と結び付け先の規則は次のとおりである。

- 結び付けは `write:task` を持つ場合だけ行う。セッションは常に持つ。`write:drive` だけの PAT では保存そのものは通し、同期を飛ばして既存の `body` 行にも触れない
- 抽出は GitHub 連携と同じ `service::forge::task_refs` を使う。キーは大小文字不問で、`Closes` などのキーワードは無視する
- 拾う前にフェンスコードブロック（```` ``` ```` / `~~~`）とインラインコードの中身を落とす。閉じられていないフェンスは末尾まで落とす。インラインコードは行内でだけ対にする
- 解決はファイルのテナント内のプロジェクトだけで行う。見つからない参照・削除済みタスクは捨てる
- 結び付け先は手動の添付と同じ範囲である。テナント一般ファイル（`project_id` が NULL）はテナント内のどのプロジェクトのタスクにも結ぶ。プロジェクトファイルは同じプロジェクトのタスクにだけ結び、別プロジェクトの参照はリンクにせず黙って捨てる
- 結び付けるのは、保存した利用者が入れるプロジェクトのタスクだけである。テナントオーナーは全プロジェクトに入れる。入れないものは黙って捨てる
- 1 ファイルあたり、本文に現れた順で最初の 200 件の一意な参照までとする。解決できない参照も数える。200 件に達した時点で走査を止め、超えた分は捨てる

行の更新の規則は次のとおりである。

- 今回の集合にあって行が無ければ `body` で作る。`manual` の行は書き換えない。今回の集合に無い `body` の行は消し、`manual` の行は残す
- 消すのも、保存した利用者が入れるプロジェクトの `body` 行と削除済みタスクの行だけである。入れないプロジェクトへの `body` 行は残す
- プロジェクトファイルから別プロジェクトのタスクへの `body` 行は、保存者が入れるかに関わらず消す
- ファイルをプロジェクトのフォルダへ移すと、移動と同じトランザクションで、移動先のプロジェクトの外のタスクへの `body` 行を消す。`PATCH .../files/{id}` の `folder_id` と、フォルダの移動で配下の `project_id` が変わる場合の両方が対象である。`manual` の行と、テナント一般のフォルダへ移した場合の行は残す

`GET /v1/tenants/{tenant_id}/drive/files/{id}/tasks` は、ファイルに結び付いたタスク（`manual` と `body` の両方）を返す。呼び出し側が入れないプロジェクトのタスクと削除済みタスクは含めない。

## データ

`drive_files` の列は次のとおりである。

| 列 | 型 | 制約 | 説明 |
|---|---|---|---|
| `id` | UUID | PK | |
| `name` | VARCHAR | NOT NULL | ファイルの表示名（元ファイル名） |
| `size` | BIGINT | NOT NULL | バイト数 |
| `mime_type` | VARCHAR | NOT NULL | `application/octet-stream` など |
| `storage_type` | VARCHAR(16) | NOT NULL | Rust 側では enum。`s3` または `local` |
| `storage_key` | VARCHAR | NOT NULL | ストレージ固有のキー（UUID v4）。S3 の key またはローカルの相対パス |
| `tenant_id` | UUID | NOT NULL, FK tenants CASCADE | |
| `project_id` | UUID | NULL 可, FK projects CASCADE | フォルダの `project_id` を非正規化した値。ファイルのフォルダ移動時に再設定する |
| `uploader_id` | UUID | NOT NULL, FK users | |
| `folder_id` | UUID | NULL 可, FK drive_folders SET NULL | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `content_hash` | VARCHAR(64) | NULL 可 | 本文の SHA-256（hex）。アップロードと本文の更新で設定し、名前変更・移動では変えない。導入より前の行は backfill せず NULL のまま |

`project_id` がセットされているときは `folder_id` も非 NULL でなければならない。DB の CHECK 制約（`project_id IS NULL OR folder_id IS NOT NULL`）と entity の `before_save` の両方で強制する。`url` の列は持たず、API の応答を作るときに URL を組み立てる。

列の定義は [m20260520000000_initial_schema.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20260520000000_initial_schema.rs)、`content_hash` は [m20261016005000_drive_files_content_hash.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261016005000_drive_files_content_hash.rs)、`task_attachments.source` は [m20261016010000_task_attachments_source.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261016010000_task_attachments_source.rs) にある。

## API

ファイルのエンドポイントは次のとおりである。

| メソッド | パス | 説明 |
|---|---|---|
| GET | `/v1/tenants/{tenant_id}/drive/files` | ファイルの一覧 |
| POST | `/v1/tenants/{tenant_id}/drive/files` | ファイルのアップロード |
| GET | `/v1/tenants/{tenant_id}/drive/files/{id}` | ファイルのメタデータの取得 |
| PATCH | `/v1/tenants/{tenant_id}/drive/files/{id}` | ファイルの更新（名前・フォルダ移動） |
| PUT | `/v1/tenants/{tenant_id}/drive/files/{id}/content` | 編集可能なテキストファイルの本文の更新 |
| DELETE | `/v1/tenants/{tenant_id}/drive/files/{id}` | ファイルの削除 |
| GET | `/v1/tenants/{tenant_id}/drive/files/{id}/tasks` | ファイルに結び付いたタスクの一覧 |
| GET | `/v1/drive/files/{id}/content` | ファイルの内容の配信（ローカル・S3 プロキシ） |
| GET | `/v1/drive/files/{id}/content?token={token}` | 公開リンクのトークンによるファイルの配信 |

タスク添付のエンドポイントは次のとおりである。

| メソッド | パス | スコープ | 説明 |
|---|---|---|---|
| GET | `/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments` | `read:task` | 添付の一覧 |
| POST | `/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments` | `write:task` | `{ "drive_file_id": "uuid" }` を添付する |
| DELETE | `/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments/{attachment_id}` | `write:task` | 添付を解除する |

## 認可

アクセスの規則とエンドポイントごとのスコープは[Drive の認可](/features/drive#認可)の節に書く。このページに固有の規則は次のとおりである。

| 操作 | 誰が | スコープ |
|---|---|---|
| 添付の解除 | 添付を作成した利用者またはテナントオーナー | `write:task` |
| 結び付いたタスクの一覧 | メタデータの取得（`GET .../drive/files/{id}`）と同じ判定を通る人。閲覧できなければ `403` / `404` | `read:drive` と `read:task` |
| `body` 行の同期 | 保存した利用者 | `write:task` がなければ同期を飛ばす |

## 画面

タスク詳細の添付欄（`TaskAttachments.vue`）から Drive のファイルを添付する。OpenAPI クライアント用の定義と PAT のスコープ表示がある。

## 未実装と既知の制限

- 画像のサムネイル生成が無い
- ファイルの全文検索が無い
- Drive のページ、ファイルブラウザ、クォータ、共有の画面が無い
- 個別削除は DB の削除後のストレージエラーを無視する。孤児オブジェクトの扱いは[Drive の未実装と既知の制限](/features/drive#未実装と既知の制限)に書く

### ファイルブラウザの導入時の設計

ページもコンポーネントもまだ無い。以下は導入時の設計で、URL は [URL 仕様](/frontend/url-spec)に合わせた予定のものである。

| URL | 内容 |
|---|---|
| `/{tenant}/drive` | ドライブのトップ（ルートフォルダ） |
| `/{tenant}/drive/{folder_id}` | フォルダ内 |

上部にパンくず、左にフォルダツリーと新規フォルダのボタン、右に検索欄、アップロード、表示切り替え、ファイル一覧を置く。

![ドライブ画面の配置。上部にパンくず、左にフォルダツリー、右に検索欄とファイル一覧を置く](/diagrams/drive-layout.svg)

コンポーネントの構成は次のとおりである。

| コンポーネント | ファイル | 説明 |
|---|---|---|
| `DrivePage` | `src/pages/@tenant/drive/+Page.vue` | ドライブ全体 |
| `DriveFolderPage` | `src/pages/@tenant/drive/@folderId/+Page.vue` | フォルダ内 |
| `DriveSidebar` | `src/components/drive/DriveSidebar.vue` | フォルダツリー |
| `DriveFileGrid` | `src/components/drive/DriveFileGrid.vue` | ファイル一覧（グリッド） |
| `DriveFileList` | `src/components/drive/DriveFileList.vue` | ファイル一覧（リスト） |
| `DriveUploadButton` | `src/components/drive/DriveUploadButton.vue` | ドラッグ&ドロップ対応 |
| `DriveFileCard` | `src/components/drive/DriveFileCard.vue` | ファイルカード（グリッド用） |
| `DriveFileDetail` | `src/components/drive/DriveFileDetail.vue` | 詳細パネル（サイドシート） |

主な操作は次のとおりである。

- アップロードは、ボタンのクリックまたはエリアへのドラッグ&ドロップで行い、プログレスバーを表示する
- フォルダの作成は、サイドバーの「+ 新しいフォルダ」ボタンからインラインで入力する
- ファイルの詳細は、ファイルカードのクリックで右サイドシートに表示し、URL をコピーできる
- 削除は、右クリックメニューまたは詳細パネルの削除ボタンから確認ダイアログを経て行う

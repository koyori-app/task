---
title: Drive のファイル
description: Drive のファイルのアップロード・一覧・本文の更新と配信、タスクへの添付と本文からの参照、本文の索引と検索
status: partial
updated: 2026-11-21
slug: dev/features/drive/files
sidebar:
  order: 1
---

## 概要

テナントの利用者が Drive にファイルをアップロードし、一覧・更新・削除・ダウンロードする機能である。Markdown の本文に書いた `KEY-N` をタスクに結び付け、タスクへ手動で添付することもできる。アクセスの規則とストレージは[Drive の概要](/dev/features/drive)に書き、判断の理由は [Drive の決定の記録](/dev/features/drive#決定の記録)に集める。フォルダは[フォルダと共有](/dev/features/drive/folders-and-sharing)、容量は[クォータ](/dev/features/drive/quota)、プロジェクトのドキュメント画面は[ドキュメント](/dev/features/drive/documents)に書く。

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

`GET /v1/drive/files/{id}/content` は本文を `200 OK` で返す。`?token={share_token}` を付けると公開リンクのトークンで配信する。誰が取得できるかは[Drive の認可](/dev/features/drive#認可)の節に書く。

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

規則の全体は[タスク拡張](/dev/features/tasks/extensions)のファイル添付に書く。

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

### 本文の索引

`mime_type` が `text/markdown` のファイルは、本文を見出しごとの節に切って `drive_file_sections` に保存する。対象はテナント内の全ファイルで、サブフォルダのファイルとテナント一般ファイルも含む。名前が `.md` で終わるかは見ない。

索引を作り直すのは、アップロード（`POST .../drive/files`）と本文の更新（`PUT .../drive/files/{id}/content`）である。DB の更新と同じトランザクションで、そのファイルの行を全部消して入れ直す。`write:task` の有無に依らないので、`write:drive` だけの PAT の保存でも作り直す。

節の切り方は次のとおりである。

- Markdown として解釈し（`pulldown-cmark`）、引用とリストの外に書かれた見出しを節の境界にする。ATX（`# 題`）と Setext（下線）の両方を拾う
- フェンスコードブロックの中の `#` と、先頭の frontmatter（`---` で囲んだ YAML）の中身は見出しにしない
- 節の本文は、見出しの行から次の境界の手前までの元の文字列である。見出しの行、コードブロック、frontmatter をそのまま残す。`KEY-N` の抽出と違い、コードの中身を落とさない
- 見出しの経路は、祖先の見出しから自分までの題の並びである。浅い見出しが来ると、それより深い見出しと同じ深さの見出しを経路から外す。題はインラインの記法を外した文字にする
- 最初の見出しより前の本文は、経路が空の節にする。空白だけなら節にしない
- 見出しが 1 つも無いファイルは、本文全体を経路が空の節 1 つにする。索引した Markdown ファイルは必ず 1 行以上を持つ

上限は次のとおりである。

| 項目 | 上限 | 超えたとき |
|---|---|---|
| 索引する本文 | 1 MiB | 先頭の 1 MiB だけを索引する。文字の途中では切らない |
| 1 ファイルの節 | 2000 | 2000 個目より後の見出しでは切らず、残りを最後の節に入れる |
| 見出し 1 つの文字数 | 200 | 超えた分を捨てる |

NUL 文字（`U+0000`）は索引に入れる前に除く。ストレージの本文は変えない。

名前変更と移動では本文を読み直さない。索引は `tenant_id` と `project_id` を持たず、検索のたびに `drive_files` と結合するので、移動したファイルは移動先のプロジェクトの検索に出る。ファイルを消すと、外部キーの CASCADE で索引も消える。

この機能より前に上げたファイルは、バックエンドの起動時に 1 回、索引の無い Markdown ファイルをストレージから読んで索引する。1 ファイルの失敗（ストレージに実体が無いなど）はログに残して次へ進み、次の起動でもう一度試す。本文を読んだ後にそのファイルの本文が更新されていれば、その保存が索引を作っているので飛ばす。

### 本文の検索

`GET /v1/tenants/{tenant_id}/projects/{project_id}/documents/search` は、そのプロジェクトのファイルの節を本文で引く。対象は `project_id` がそのプロジェクトの Markdown ファイルで、サブフォルダのファイルも含む。

| クエリ | 型 | 既定 | 内容 |
|---|---|---|---|
| `q` | string | | 検索語。前後の空白を除いて 1〜255 文字。空白で語に分け、1〜10 語。範囲の外は `400` |
| `limit` | u64 | 20 | 取得件数。`1..=100` に補正する |
| `offset` | u64 | 0 | オフセット |

当て方と並びは次のとおりである。

- 全部の語を含む節に当たる（語ごとの `ILIKE` の AND）。大文字小文字を区別しない部分一致で、`%` `_` `\` は字義どおりに扱う
- 語は節の本文と突き合わせる。見出しの行は節の本文に入るので、自分の見出しの語では当たる。祖先の見出しにだけ含まれる語では当たらない
- 表記ゆれ（`pg_trgm` の `<%`）は見ない。`pg_trgm` は `ILIKE` を速くする GIN 索引としてだけ使う
- 並びはファイル名の昇順、ファイルの `id` の昇順、節の位置の昇順である。関連度では並べない

応答は `hits` と `total` である。`total` は当たった節の数（`limit` と `offset` を掛ける前）である。認可はプロジェクトの単位で決まるので、呼び出し側が見られない節は数に入らない。`hits` の 1 件は節 1 つで、1 ファイルから複数の節が返る。

| フィールド | 内容 |
|---|---|
| `file_id` / `file_name` / `folder_id` | 節のあるファイルの `id`、名前、フォルダ |
| `heading_path` | 見出しの経路（祖先から自分まで）。最初の見出しより前の節は空の配列 |
| `position` | ファイルの中の節の順（0 始まり） |
| `excerpt` | 抜粋。語を順に探し、最初に見つかった語の前後 100 文字を切り出す。切った側に `…` を付ける。どの語も見つからなければ先頭の 200 文字。素の文字列で、HTML にもエスケープもしない |
| `updated_at` | ファイルの `updated_at` |

本文の全体は `GET /v1/drive/files/{file_id}/content` で取る。

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

`drive_file_sections` は Markdown ファイルの本文の索引で、列は次のとおりである。

| 列 | 型 | 制約 | 説明 |
|---|---|---|---|
| `id` | UUID | PK | |
| `drive_file_id` | UUID | NOT NULL, FK drive_files CASCADE | |
| `position` | INTEGER | NOT NULL | ファイルの中の節の順（0 始まり） |
| `heading_path` | VARCHAR[] | NOT NULL | 見出しの経路（祖先から自分まで） |
| `body` | TEXT | NOT NULL | 節の本文。`pg_trgm` の GIN 索引（`idx_drive_file_sections_body_trgm`）を張る |

`(drive_file_id, position)` は一意である。`tenant_id` と `project_id` は持たない。版と差分も持たず、保存のたびにそのファイルの行を入れ直す。定義は [m20261121000000_drive_file_sections.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261121000000_drive_file_sections.rs) にある。

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
| GET | `/v1/tenants/{tenant_id}/projects/{project_id}/documents/search` | プロジェクトの Markdown ファイルの本文検索 |

タスク添付のエンドポイントは次のとおりである。

| メソッド | パス | スコープ | 説明 |
|---|---|---|---|
| GET | `/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments` | `read:task` | 添付の一覧 |
| POST | `/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments` | `write:task` | `{ "drive_file_id": "uuid" }` を添付する |
| DELETE | `/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments/{attachment_id}` | `write:task` | 添付を解除する |

## 認可

アクセスの規則とエンドポイントごとのスコープは[Drive の認可](/dev/features/drive#認可)の節に書く。このページに固有の規則は次のとおりである。

| 操作 | 誰が | スコープ |
|---|---|---|
| 添付の解除 | 添付を作成した利用者またはテナントオーナー | `write:task` |
| 結び付いたタスクの一覧 | メタデータの取得（`GET .../drive/files/{id}`）と同じ判定を通る人。閲覧できなければ `403` / `404` | `read:drive` と `read:task` |
| `body` 行の同期 | 保存した利用者 | `write:task` がなければ同期を飛ばす |
| 本文の検索 | そのプロジェクトに入れる人（テナントに所属し、プロジェクトの公開規則を満たす人）とテナントオーナー。PAT はテナントとプロジェクトの束縛も満たすこと | `read:drive` |

本文の検索は、ファイルの閲覧より狭い。次の人は本文を読めても検索はできず、`403` を返す。

- フォルダの利用者指定共有だけで見える人
- 公開リンクのトークンの提示者（検索はトークンを受け取らない）
- テナントに所属しないプロジェクト限定のゲスト

テナント一般ファイルは、どのプロジェクトの検索にも出ない。

## 画面

タスク詳細の添付欄（`TaskAttachments.vue`）から Drive のファイルを添付する。OpenAPI クライアント用の定義と PAT のスコープ表示がある。

### 入力欄への貼り付け

本文の入力欄にファイルを貼り付ける（Ctrl+V / ⌘V）と、Drive へアップロードして本文に 1 行を入れる。

| 項目 | 内容 |
|---|---|
| 対象の入力欄 | タスクの説明、タスク作成ダイアログの説明、ドキュメントの本文、コメントの入力欄、コメントの編集 |
| 保存先 | 入力欄があるプロジェクトのルートフォルダ。タスクの添付と同じ場所で、読める人はプロジェクトのメンバーである |
| 上限 | 貼り付け専用の上限は無い。サーバーの `UPLOAD_MAX_SIZE_MB` と容量に従う |
| 複数のファイル | 1 回の貼り付けのファイルを並行でアップロードし、貼り付けた順に 1 行ずつ入れる |

クリップボードにファイルがあり、`text/plain` と `text/html` の少なくとも片方が空のときだけファイルとして扱う。表計算や文書のアプリのように文字と HTML と画像を同時に載せる貼り付けは、文字として貼る。

本文に入る行は次のとおりである。URL は API の相対パス `/v1/drive/files/{id}/content` で、web は描画するときに `/api` を前に足す。

| ファイル | 本文に入る行 | 表示 |
|---|---|---|
| MIME タイプが `image/` で始まる、または拡張子が `heic` `heif` | `![名前](URL)` | 画像 |
| 拡張子が `mp4` `mov` `webm` `m4v` | `![名前](URL)` | 動画のプレイヤー |
| 拡張子が `mp3` `wav` `m4a` `ogg` `caf` `aac` | `![名前](URL)` | 音声のプレイヤー |
| それ以外 | `[名前](URL)` | ダウンロードのリンク |

名前に含まれる Markdown の記号（`\` `[` `]` `*` `` ` `` `<` `&` `~` と、語の端の `_`）は `\` を前に付けて入れ、名前が強調・取り消し線・タグとして解釈されないようにする。

描画の規則は次のとおりである。

- 書き換えるのは、URL が `/v1/drive/files/{UUID}/content` に完全一致する画像とリンクだけである。クエリやパスが続くもの、絶対 URL は書き換えない
- 動画と音声は、画像の記法の名前（`alt`）の拡張子で見分ける。本文の URL には拡張子も MIME タイプも無い。拡張子の大文字と小文字は区別しない。名前から拡張子を消すと画像として描く
- プレイヤーは自動再生しない
- ブラウザが画像・動画・音声を読み込めなかったときは、その場でダウンロードのリンクに替える。HEIC は Safari 以外で、`.mov` は中身のコーデックによって表示できないことがある
- リンクの中に置いた画像（`[![名前](URL)](リンク先)`）は URL を替えるだけで、プレイヤーにもダウンロードのリンクにもしない
- ダウンロードのリンクには `download` 属性を付ける

アップロードの間と失敗時の扱いは次のとおりである。

- アップロードの間は、貼り付けた位置に「アップロード中…」とファイル名を表示する。この表示は本文に含まれず、下書きにも保存にも入らない
- アップロードの間は、その入力欄の送信・保存・作成を止める。表示の「取り消し」でアップロードを中断でき、中断すると送信・保存・作成をまた行える
- 終わると、表示があった位置に行を入れる。その間に前後へ文字を入力していても位置は追従する
- 失敗すると、同じ位置に理由と「再試行」「取り消し」を表示する。複数のうち一部だけ失敗したときは、成功した行だけを入れ、失敗したファイルだけを再試行の対象に残す
- 入力欄を閉じる、別のタスクやスレッドへ切り替える、本文が外から差し替わる、表示を含む範囲を消す（元に戻す操作で消える場合を含む）、のいずれかで進行中のアップロードを中断し、結果を捨てる。送信・保存・作成はまた行える
- 行の入れ替え（Alt+↑ / Alt+↓）と前後の字の入れ替え（Ctrl+T）では中断しない。表示は元の行の同じ位置に付いていく

失敗の理由は次の文言で表示する。

| 条件 | 文言 |
|---|---|
| 0 バイトのファイル | 空のファイルは貼り付けできません |
| ルートフォルダの取得に失敗（通信の切断を含む） | 保存先を読み込めませんでした |
| プロジェクトにルートフォルダが無い | プロジェクトの保存先が見つかりません |
| `413`（サイズまたは容量の超過） | ファイルがサイズ上限または保存容量を超えています |
| それ以外の失敗（通信の切断を含む） | ファイルをアップロードできませんでした |

## 未実装と既知の制限

- 貼り付けたファイルは、本文から行を消しても、結果を捨てた場合でも Drive に残る
- 貼り付けたファイルはタスクの添付にならない
- アップロードの間に行の入れ替えを元に戻す操作をすると、表示のある行が消されて入れ直され、アップロードを中断することがある
- 配信が Range リクエストに対応していない（TASK-565）。対応するまで、Safari は動画と音声を再生できずダウンロードのリンクに替わり、他のブラウザでも未読み込みの位置へシークできない
- 貼り付けたファイルを変換しない。HEIC の画像や HEVC の動画は、表示できないブラウザではダウンロードのリンクになる
- 画像のサムネイル生成が無い
- 本文で検索できるのは Markdown ファイルだけである。他の種類のファイルとテナント一般ファイルは検索できない
- テナントを横断する本文検索、複数のプロジェクトをまとめた本文検索が無い
- 本文の検索は、祖先の見出しにだけ含まれる語では当たらず、関連度で並べない。3 文字未満の語では GIN 索引が使われない
- 本文の検索は 1 MiB を超えた分と、起動時の索引に失敗したファイルを引けない
- 本文の検索を使う画面が無い
- Drive のページ、ファイルブラウザ、クォータ、共有の画面が無い
- 個別削除は DB の削除後のストレージエラーを無視する。孤児オブジェクトの扱いは[Drive の未実装と既知の制限](/dev/features/drive#未実装と既知の制限)に書く

### ファイルブラウザの導入時の設計

ページもコンポーネントもまだ無い。以下は導入時の設計で、URL は [URL 仕様](/dev/frontend/url-spec)に合わせた予定のものである。

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

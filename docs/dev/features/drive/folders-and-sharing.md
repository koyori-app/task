---
title: Drive のフォルダと共有
description: Drive の階層フォルダ、プロジェクトフォルダの扱い、利用者指定と公開リンクによるフォルダ共有
status: partial
updated: 2026-11-04
slug: dev/features/drive/folders-and-sharing
sidebar:
  order: 2
---

## 概要

アップロードしたファイルを階層フォルダで整理し、フォルダを特定の利用者や公開リンクで共有する機能である。プロジェクトには同名のルートフォルダが自動で作られる。アクセスの規則は [Drive の認可](/dev/features/drive#認可)の節に書き、判断の理由は [Drive の決定の記録](/dev/features/drive#決定の記録)に集める。

## 用語

| 語 | 意味 |
|---|---|
| プロジェクトフォルダ | `project_id` がセットされたフォルダ |
| プロジェクトルート | プロジェクトの作成時に自動で作られる、プロジェクトのルートフォルダ |
| 利用者指定共有 | 特定の利用者への共有（`type: "user"`） |
| 公開リンク共有 | `share_token` を持つ URL による共有（`type: "public_link"`） |

## 振る舞い

### プロジェクトフォルダのライフサイクル

プロジェクト（個人プロジェクトを含む）の作成時に同名のフォルダを `drive_folders` に自動で作り、`project_id` を紐付ける。

| イベント | 動作 |
|---|---|
| プロジェクトの作成 | `drive_folders` にプロジェクトフォルダを自動で作る |
| プロジェクトの削除 | `project_id` の外部キーでフォルダ・ファイルの DB の行を CASCADE で削除する |
| 子フォルダの作成 | 親フォルダの `project_id` を継承し、親がプロジェクト配下ならそのプロジェクトへのアクセス権を確認する |
| フォルダの移動 | 移動元・移動先のプロジェクト権限を確認し、フォルダと配下の全フォルダ・ファイルの `project_id` を同一トランザクションで同期する |
| プロジェクトルートの移動・削除 | 通常のフォルダ API では空でも `409 Conflict` を返す。プロジェクトの削除による CASCADE に限る |
| ファイルをプロジェクトフォルダへ移動 | `drive_files.project_id` を移動先フォルダの `project_id` に更新する |
| ファイルをプロジェクトフォルダ外へ移動 | `drive_files.project_id` を `NULL` にリセットする |

作成・移動とフォルダの削除はテナント単位の Drive のロックで直列化し、ロック取得後に階層とプロジェクト権限を確認する。ファイルの移動でも移動元・移動先の権限が必要である。本文の更新とファイルの削除は対象ファイルの行ロック取得後に再認可する。既存データの `project_id` の不整合は [m20260904000000_drive_project_id_backfill.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20260904000000_drive_project_id_backfill.rs) で補正する。

### フォルダの一覧

`GET /v1/tenants/{tenant_id}/drive/folders` のクエリは次のとおりである。クエリをすべて省略すると、テナントの全フォルダを返す。プロジェクトのルートフォルダ（ドキュメント置き場）は `project_id` と `root_only=true` の組で 1 件取る。

| クエリ | 型 | 既定 | 内容 |
|---|---|---|---|
| `project_id` | UUID? | | そのプロジェクトのフォルダだけを返す。指定時はそのプロジェクトへのアクセス権を確認し、無ければ `403` |
| `parent_id` | UUID? | | そのフォルダの直下だけを返す。省略時は全階層 |
| `root_only` | bool | `false` | `true` でルート（`parent_id IS NULL`）だけを返す。`parent_id` との同時指定は `400` |

### フォルダの作成・更新・削除

作成時の `parent_id` で親フォルダを指定し、親の `project_id` を継承する。更新時の `parent_id` は省略で変更なし、明示的な `null` でドライブ直下への移動を表す。移動元・移動先の権限の確認、配下の同期とルートの保護は[プロジェクトフォルダのライフサイクル](#プロジェクトフォルダのライフサイクル)に従う。自身や子孫を親に指定した場合と、空の `name` は `400` を返す。

削除の挙動は次のとおりである。

- 直下にファイルまたは子フォルダが存在する場合は `409 Conflict` を返し、削除しない
- プロジェクトルートは空でも `409 Conflict` を返す
- 権限のないプロジェクト配下の操作は先に `403` で拒否する

### フォルダの共有

共有は利用者指定と公開リンクの 2 種類である。フォルダを共有すると、その配下のサブフォルダ・ファイルすべてにアクセス権が再帰的に及ぶ。

権限のレベルは次のとおりである。`viewer` の共有では削除・アップロードの API を `403` で止める。

| `permission` | できること |
|---|---|
| `viewer` | ファイル・フォルダの閲覧・ダウンロード |

`POST .../shares` の本文の規則は次のとおりである。

| `type` | 必須の項目 | 任意の項目 |
|---|---|---|
| `user` | `user_id`、`permission` | `expires_at`（`null` は無期限） |
| `public_link` | `permission` | `expires_at`（`null` は無期限） |

作成の規則は次のとおりである。

- 応答に `share_url` は含めない。クライアントが `window.location.origin + "/drive/share/" + share_token` で組み立てる
- `share_token` は URL-safe な 32 文字のランダムな文字列である
- `permission: "editor"` を指定した場合は `422 Unprocessable Entity` を返す。それ以外の未知の値と、未知の `type`、`user_id` の無い `type: "user"` は `400 Bad Request` を返す

### 公開リンク

`GET /v1/drive/share/{token}` は認証なしでフォルダのメタデータ（名前、作成者名、直下のファイル数）を返す。

- 不明なトークンは `404 Not Found` を返す
- 有効期限切れの場合は `410 Gone` を返す。ファイル本文の API は期限切れに `403` を返す。`expires_at` は毎回確認する

`GET /v1/drive/share/{token}/files` は同じトークンの検証を行い、共有フォルダ直下の `DriveFile` の配列を返す。子フォルダは列挙しない。返す `url` にトークンは付かないので、共有リンクから本文を取得するクライアントは `?token={share_token}` を付ける。

### 公開共有のレートリミット

公開共有の API（`/share/{token}`・`/share/{token}/files`・`?token=` 付きの本文取得）は、接続元 IP × トークン指紋（SHA-256 の先頭 16 hex）ごとに固定窓の 60 秒 120 回までである。

- 3 つのエンドポイントの合計で数える。別トークン・別 IP・認証済みの経路は巻き込まない
- 同じトークンへの負荷を抑える層であり、異なるトークンの試行は数えない
- 超過は `429 Too Many Requests` と `Retry-After` を返す
- カウンターは Redis に置き、平文のトークンはキーに使わない
- 値は定数で、環境変数は設けない

共有トークンの総当たりは、32 文字 URL-safe ランダムのエントロピー（192 bit）で防ぐ。

## データ

`drive_folders` の列は次のとおりである。

| 列 | 型 | 制約 | 説明 |
|---|---|---|---|
| `id` | UUID | PK | |
| `name` | VARCHAR | NOT NULL | フォルダ名 |
| `parent_id` | UUID | NULL 可, FK self SET NULL | 親フォルダ。ルートは NULL |
| `tenant_id` | UUID | NOT NULL, FK tenants CASCADE | |
| `project_id` | UUID | NULL 可, FK projects CASCADE | セットされているとプロジェクトフォルダになる |
| `created_by` | UUID | NOT NULL, FK users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

`drive_folder_shares` の列は次のとおりである。

| 列 | 型 | 制約 | 説明 |
|---|---|---|---|
| `id` | UUID | PK | |
| `folder_id` | UUID | NOT NULL, FK drive_folders CASCADE | 共有するフォルダ |
| `shared_with_user_id` | UUID | NULL 可, FK users CASCADE | 特定の利用者への共有。NULL の場合は公開リンク |
| `share_token` | VARCHAR | NULL 可, UNIQUE | 公開リンクのトークン（URL-safe なランダムな文字列）。NULL の場合は利用者指定共有 |
| `permission` | VARCHAR(16) | NOT NULL | Rust 側では enum。値は `viewer` のみ |
| `created_by` | UUID | NOT NULL, FK users | 共有を作成した利用者 |
| `expires_at` | TIMESTAMPTZ | NULL 可 | 有効期限。NULL は無期限 |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

`shared_with_user_id` と `share_token` はどちらか一方のみ設定する（CHECK 制約）。両方 NULL または両方 NOT NULL は不正である。

列の定義は [m20260520000000_initial_schema.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20260520000000_initial_schema.rs) にある。

## API

フォルダのエンドポイントは次のとおりである。

| メソッド | パス | 説明 |
|---|---|---|
| GET | `/v1/tenants/{tenant_id}/drive/folders` | フォルダの一覧 |
| POST | `/v1/tenants/{tenant_id}/drive/folders` | フォルダの作成 |
| PATCH | `/v1/tenants/{tenant_id}/drive/folders/{folder_id}` | フォルダの更新（名前変更・移動） |
| DELETE | `/v1/tenants/{tenant_id}/drive/folders/{folder_id}` | フォルダの削除 |

共有のエンドポイントは次のとおりである。

| メソッド | パス | 説明 |
|---|---|---|
| GET | `/v1/tenants/{tenant_id}/drive/folders/{folder_id}/shares` | 共有の一覧 |
| POST | `/v1/tenants/{tenant_id}/drive/folders/{folder_id}/shares` | 共有の作成 |
| DELETE | `/v1/tenants/{tenant_id}/drive/folders/{folder_id}/shares/{share_id}` | 共有の取り消し |
| GET | `/v1/drive/share/{token}` | 公開リンクでフォルダにアクセスする（認証不要） |
| GET | `/v1/drive/share/{token}/files` | 公開リンク経由でファイルの一覧を取得する |

## 認可

エンドポイントごとのスコープは [Drive の認可](/dev/features/drive#認可)の節に書く。このページに固有の規則は次のとおりである。

| 操作 | 誰が | スコープ |
|---|---|---|
| 共有の一覧・作成・取り消し | フォルダの作成者またはテナントオーナー。プロジェクトフォルダでは、作成者が現在そのプロジェクトに入れることも条件で、外れた作成者は `403` | 一覧は `read:drive`、作成と取り消しは `write:drive` |
| 公開リンクの閲覧 | 有効なトークンを持つ人 | スコープ不要。トークン自体を認証情報として扱う |

## 未実装と既知の制限

| 優先度 | 項目 | 現状と影響 |
|---|---|---|
| 中 | 利用者指定共有の受信者が対象を見つける経路 | プロジェクトファイルの本文取得ではテナント外の共有受信者も許可できるが、ファイル・フォルダの一覧とメタデータの API が先にテナント所属を要求するので、共有受信者が対象を発見できない |
| 中 | フォルダ一覧のプロジェクト別の絞り込み | `project_id` を省略すると、テナント所属を確認した後、プロジェクト権限による絞り込みなしで全フォルダのメタデータを返す。ファイル一覧・本文の認可とは異なる |
| 中 | 公開共有の一覧の再帰性 | `/share/{token}/files` は共有フォルダ直下のファイルだけを返し、子フォルダとそのファイルを列挙しない。本文取得時のトークン判定だけは祖先方向へ継承する |
| 低 | 接続元ごとの外れトークンの数え上げ | 公開共有のレートリミットは IP × トークン指紋が鍵なので、毎回違うトークンを送る総当たりは数えない。総当たり自体はエントロピー（192 bit）で成立しないが、外れたトークンの `drive_folder_shares` の検索は毎回走る |

- フォルダ共有の `editor` 権限（`viewer` に加え、ファイルのアップロード・削除・フォルダの作成）が無く、enum にも値が無い
- 共有ページの UI が無い
- フォルダの強制削除が無い
- プロジェクトの削除でストレージの実体は連動して消えない（[Drive の未実装と既知の制限](/dev/features/drive#未実装と既知の制限)）

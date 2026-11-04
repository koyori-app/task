---
title: ドキュメント
description: プロジェクトのルートフォルダ直下の Markdown ファイルを読み書きする画面
status: implemented
updated: 2026-11-04
slug: features/drive/documents
sidebar:
  order: 4
---

## 概要

プロジェクトの「ドキュメント」は Drive の上に載せた画面で、専用の表・API は持たない。実体はプロジェクトのルートフォルダ直下の `.md` ファイルである。種類・ステータス・承認・版の履歴は持たない。ファイルの API は [Drive のファイル](/features/drive/files)、判断の理由は [Drive の決定の記録](/features/drive#決定の記録)に書く。

## 振る舞い

### 対象のファイル

対象は MIME が `text/markdown` で名前が `.md` で終わるファイルである。一覧・詳細・`+data.ts` で同じ条件を使う。

ルートフォルダは `GET .../drive/folders?project_id=<uuid>&root_only=true` の先頭 1 件で取る。無ければ一覧は空状態を出し、詳細はどのファイルも「見つかりません」にする。`folder_id` が NULL の Drive 直下と取り違えない。

### 一覧

一覧は `GET .../drive/files?folder_id=<root>&mime_type=text/markdown&sort=name_asc&limit=200` で取り、`?q=` をそのまま渡す。

- Drive の改名は MIME を変えないので、名前が `.md` で終わらないものはクライアントで外す
- `total` は外す前の数なので、件数の注記は上限で切れたとき（`total` が返った件数より多いとき）だけ出す
- 表示名はファイル名から `.md` を落としたものである

### 本文

本文は `GET /v1/drive/files/{id}/content` で取り、KFM で描画する。

- 初回はサーバの `+data.ts`、編集中のプレビューと保存後は `POST /internal/render-description` で描画する
- scope は `doc-<file_id>` で、本文の `KEY-N` はタスク詳細へのリンクになる
- 関連タスクは [本文からのタスク参照](/features/drive/files#本文からのタスク参照)の `GET .../files/{id}/tasks` で取る

### 保存

保存は `PUT .../content` に、読み込んだ時点の `DriveFile.etag` を `If-Match` で付ける。

- `412` なら「他の人が更新しました」と出し、最新を読み込んで下書きを捨てるか、自分の下書きで上書きするかを選ばせる
- どちらも最新の本文と ETag を取り直す。上書きはそこで取り直した ETag を付けて保存し直す
- タイトルの変更は `PATCH .../files/{id}` の `name`（`<タイトル>.md`）で、下書きを始めた時のタイトルから変えたときだけ送る

### 作成

新規作成は `POST .../drive/files`（multipart、`folder_id` はルート）で行う。作成時に選ぶ種類（機能仕様・API 仕様・画面仕様・議事録・手順書・メモ）は本文テンプレートの違いだけで、テンプレートはフロントの定数（`lib/documents/templates.ts`）である。

本文の選択範囲から「タスクを作成」でき、作成したタスクには手動の添付（`manual`）で結び付ける。

## 画面

| 項目 | 値 |
|---|---|
| URL | `/{tenant}/projects/{key}/docs`。詳細は [URL 仕様](/frontend/url-spec)に書く |
| 検索 | `?q=` でタイトルを検索する |

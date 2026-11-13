---
title: URL
description: フロントエンドの URL と識別子、それを定義する Vike のページファイルの対応
updated: 2026-11-12
sidebar:
  order: 1
---

フロントエンドの URL と、それを定義する Vike のページファイルの対応を示す。ルートは `apps/frontend/src/pages/` のファイルシステムルーティングで決まり、`+route` は使っていない。ルートパラメータは camelCase で書く（`@tenant`、`@projectKey`、`@taskId`、`@fileId`、`@userId`）。

## 識別子の対応

バックエンド API は UUID ベースのパスを持ち、フロントエンドの URL は人間が読める識別子を使う。

| リソース | バックエンド | フロントエンド | 例 |
|---------|------------|--------------|-----|
| テナント | UUID (`tenant_id`) | `display_id`（スラグ） | `acme` |
| プロジェクト | UUID (`project_id`) | `key`（`^[A-Z][A-Z0-9]{1,9}$`） | `ENG` |
| タスク | UUID または `KEY-N` | `KEY-N` 形式 | `ENG-42` |
| ドキュメント | UUID | UUID（Drive ファイルの ID） | `3fa85f64-...` |
| 利用者 | UUID | UUID | `3fa85f64-...` |

タスクの `KEY-N` は `seq_id` と `project.key` から組み立てる。`GET /v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}` が UUID と `KEY-N` の両方を受けるので、フロントエンドは URL の値をそのまま渡せる。

テナントの `display_id` の形式（`^[a-z0-9]+(?:-[a-z0-9]+)*$`）を検査するのは作成ダイアログだけで、バックエンドは 1 文字以上であることしか検査しない。

## URL 一覧

### 認証（テナント外）

サインイン前に開くページで、共通レイアウトの認証ガードの対象外である（`+Layout.vue` の `isAuthPage`）。

| URL | 説明 |
|-----|------|
| `/signin` | サインイン |
| `/signup` | 新規登録 |
| `/auth/reset-password` | パスワードリセット。`?token=` が無ければ申請フォーム、あれば再設定フォームを出す（[パスワードリセットと変更](/dev/features/auth/password-reset)）。メール内リンクもこの URL を指す |
| `/verify-email` | メールアドレスの確認。`?token=` はメール内リンクが付ける |
| `/desktop/authorize` | Koyori Desktop の承認（[アカウント設定](/dev/features/account-settings)）。`?port=&code_challenge=&state=&name=` を受ける。未ログインならサインイン後に戻る |
| `/invitations/accept?token=` | テナント招待の承諾（[テナントとプロジェクトの認可](/dev/design/tenant-project-authz)）。未ログインならサインイン後に戻る |

### アカウント設定（テナント外）

個人アカウントの設定はテナントに紐づかないため、`/{tenant}` の外に置く。

| URL | 説明 |
|-----|------|
| `/settings/profile` | プロフィール編集（[アカウント設定](/dev/features/account-settings)） |
| `/settings/security` | 認証方法、2 段階認証、Koyori Desktop の端末（[アカウント設定](/dev/features/account-settings)） |
| `/settings/tokens` | パーソナルアクセストークンの発行と取り消し（[アカウント設定](/dev/features/account-settings)） |
| `/settings/preferences` | 環境設定 |
| `/settings/experiments` | 実験的機能の設定 |

### ルート

| URL | 説明 |
|-----|------|
| `/` | 選択中のテナント（無ければ先頭のテナント）の `/{tenant}` へ移動する。テナントが一つも無ければワークスペースの作成を案内する |

### テナントスコープ

| URL | 説明 |
|-----|------|
| `/{tenant}` | テナントホーム（ダッシュボード） |
| `/{tenant}/my-tasks` | 自分のタスク |
| `/{tenant}/calendar` | テナントのタイムライン（[スケジュール](/dev/features/tasks/schedule)）。`?from=YYYY-MM-DD`（起点の月曜。省略時は今週）、`?member=` / `?project=`（ID。繰り返し指定可）で絞り込み、`?selected=KEY-N`（詳細のオーバーレイで開いているタスク）を持つ。URL は replaceState で書き、履歴を増やさない |
| `/{tenant}/gantt` | ガントチャート。サイドバーの入口は実験的機能の「ガントチャート」（`koyoriArcIntegration`）を有効にしたときだけ出る。URL からページを開くこと自体は設定に関係なくできる |
| `/{tenant}/settings` | テナント設定 |
| `/{tenant}/settings/members` | テナントメンバー管理 |
| `/{tenant}/users/{user_id}` | 利用者のプロフィール（閲覧のみ） |
| `/{tenant}/projects/new` | プロジェクト作成 |

### プロジェクトスコープ

`{key}` はプロジェクトキー（例: `ENG`）、`{KEY-N}` はタスク連番（例: `ENG-42`）である。

| URL | 説明 |
|-----|------|
| `/{tenant}/projects/{key}/tasks` | タスク一覧。`?view=table`（既定は List）、`?page=`、`?q=`、`?label=`、`?sort=列:asc,列:desc`、`?selected=KEY-N`（分割ビューで選択中のタスク）を持ち、既定値は URL から省く。URL は replaceState で書く |
| `/{tenant}/projects/{key}/tasks/{KEY-N}` | タスク詳細 |
| `/{tenant}/projects/{key}/docs` | ドキュメント一覧（[Drive](/dev/features/drive)のドキュメント画面）。`?q=` でタイトル検索 |
| `/{tenant}/projects/{key}/docs/{file_id}` | ドキュメント詳細。`{file_id}` は Drive ファイルの UUID。`?edit=1` で編集モード |
| `/{tenant}/projects/{key}/settings` | プロジェクト設定。メンバー管理もこの画面の節である |
| `/{tenant}/projects/{key}/reviews` | [レビュー指摘](/dev/features/review-findings)の画面。`?pr=618`、`?round=`、`?severity=`、`?state=`、`?finding=`、`?mine=1`、`?page=` を持つ |

## ページファイル

新しい URL を追加または変更するときは、この表も同じ PR で更新する。パスはすべて `apps/frontend/src/pages/` からの相対パスである。

| URL パターン | Vike ページファイル |
|-------------|-------------------|
| `/` | `index/+Page.vue` |
| `/signin` | `signin/+Page.vue` |
| `/signup` | `signup/+Page.vue` |
| `/verify-email` | `verify-email/+Page.vue` |
| `/auth/reset-password` | `auth/reset-password/+Page.vue` |
| `/desktop/authorize` | `desktop/authorize/+Page.vue` |
| `/invitations/accept` | `invitations/accept/+Page.vue` |
| `/settings/profile` | `settings/profile/+Page.vue` |
| `/settings/security` | `settings/security/+Page.vue` |
| `/settings/tokens` | `settings/tokens/+Page.vue` |
| `/settings/preferences` | `settings/preferences/+Page.vue` |
| `/settings/experiments` | `settings/experiments/+Page.vue` |
| `/{tenant}` | `@tenant/+Page.vue` |
| `/{tenant}/my-tasks` | `@tenant/my-tasks/+Page.vue` |
| `/{tenant}/calendar` | `@tenant/calendar/+Page.vue` |
| `/{tenant}/gantt` | `@tenant/gantt/+Page.vue` |
| `/{tenant}/settings` | `@tenant/settings/+Page.vue` |
| `/{tenant}/settings/members` | `@tenant/settings/members/+Page.vue` |
| `/{tenant}/users/{user_id}` | `@tenant/users/@userId/+Page.vue` |
| `/{tenant}/projects/new` | `@tenant/projects/new/+Page.vue` |
| `/{tenant}/projects/{key}/tasks` | `@tenant/projects/@projectKey/tasks/+Page.vue` |
| `/{tenant}/projects/{key}/tasks/{KEY-N}` | `@tenant/projects/@projectKey/tasks/@taskId/+Page.vue` |
| `/{tenant}/projects/{key}/docs` | `@tenant/projects/@projectKey/docs/+Page.vue` |
| `/{tenant}/projects/{key}/docs/{file_id}` | `@tenant/projects/@projectKey/docs/@fileId/+Page.vue` |
| `/{tenant}/projects/{key}/settings` | `@tenant/projects/@projectKey/settings/+Page.vue` |
| `/{tenant}/projects/{key}/reviews` | `@tenant/projects/@projectKey/reviews/+Page.vue` |
| （エラー） | `_error/+Page.vue` |

URL を持たないファイルは次のとおりである。

| ファイル | 役割 |
|---|---|
| `+Layout.vue` | 共通レイアウト（認証ガード、ヘッダー、サイドバー、パンくず） |
| `+Head.vue` | head |
| `+client.ts` | client 専用 entry |
| `+config.ts` | Vike の設定 |
| `@tenant/projects/@projectKey/tasks/@taskId/+data.ts` | 説明の KFM 描画（サーバー） |
| `@tenant/projects/@projectKey/docs/@fileId/+data.ts` | 本文の KFM 描画（サーバー） |

`+Layout.vue` はルート直下の一つだけで、テナント用やプロジェクト用のレイアウトは無い。

## 移行済みの URL

仮ページの `/labels` と `/tasks` はプロジェクトスコープへ移した。その後、プロジェクト配下の表示専用のラベルページは削除した（ラベルの管理は設定のラベル節にある）。いずれもページは残っておらず、転送はしない。

| 旧 URL | 移行先 |
|--------|--------|
| `/labels` | `/{tenant}/projects/{key}/settings?section=labels` |
| `/tasks` | `/{tenant}/projects/{key}/tasks` |
| `/{tenant}/projects/{key}/labels` | `/{tenant}/projects/{key}/settings?section=labels` |

## 未実装と既知の制限

次の URL は予定として残しているが、対応するページが無い。開くと `_error/+Page.vue` のエラーページになる。

| URL | 予定していた内容 | 現状 |
|-----|-----------------|------|
| `/{tenant}/drive`、`/{tenant}/drive/{folder_id}` | ドライブのファイルブラウザ。フォルダの識別子は UUID | 画面は未実装（[Drive](/dev/features/drive)）。API はある |
| `/{tenant}/projects` | プロジェクト一覧 | ページなし。プロジェクトへの入口はサイドバーである |
| `/{tenant}/projects/{key}` | プロジェクトホーム | ページなし。プロジェクトの入口は `/tasks` である |
| `/{tenant}/projects/{key}/milestones` | マイルストーン一覧 | ページなし |
| `/{tenant}/projects/{key}/members` | プロジェクトメンバー | ページなし。`settings` の節で扱う |
| `/admin`、`/admin/users`、`/admin/tenants`、`/admin/audit-logs` | 管理者画面（`is_admin = true` の利用者のみ） | 画面は未実装（[サービス管理者](/dev/features/admin)） |

テナント直下のルートと `/signin` などの固定ルートは同じ階層にある。`display_id` に予約語の検査は無いので、固定ルートと同じ名前のテナントを作ると、そのテナントのホームと固定ルートが同じ URL になる。

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-05-31 | テナントの識別子は `display_id`（スラグ）にする | UUID より読みやすい |
| 2026-05-31 | プロジェクトの識別子は `key`（大文字英数）にする | バックエンドが `validate_project_key` で形式を固定しており、テナント内で一意である |
| 2026-05-31 | タスクの識別子は `KEY-N` 形式にする | API が直接受ける（`ENG-42`） |
| 2026-05-31 | `/projects/` セグメントを保持する | `/{tenant}/settings` などのテナント固定ルートとプロジェクトキーが衝突しないよう分離する |
| 2026-05-31 | ドライブのフォルダの識別子は UUID にする | フォルダにスラグが無い |
| 2026-05-31 | 管理者ページは `/admin/*` に分離する | `is_admin` フラグで保護し、テナントスコープから独立させる |
| 2026-08-19 | アカウント設定は `/settings/*` に分離する | テナントに紐づかない |
| 2026-11-04 | パスワードリセットの URL を `/forgot-password` と `/reset-password` の二つに分けず、`/auth/reset-password` の一つにまとめる | 記録なし（日付は不明） |
| 2026-11-11 | プロジェクト配下の `/labels` ページを消し、転送は置かない | 表示専用でラベル管理は設定の節に重複していた。ページへの導線はサイドバーだけで、既存の `/labels` `/tasks` の廃止と同じく転送を持たない |

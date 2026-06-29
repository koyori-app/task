---
title: マイグレーション / スキーマファースト運用
description: SeaORM エンティティをスキーマファーストで運用するためのマイグレーション手順とエンティティ再生成フロー
icon: lucide:database
---

# マイグレーション / スキーマファースト運用

> ステータス: **運用中**
> 作成日: 2026-06-29

---

## 1. 概要

バックエンドの DB スキーマとエンティティは **スキーマファースト** で運用する。

- **スキーマの正本** は `apps/backend/migration` crate（SQL マイグレーション）にある。
- **エンティティ** (`apps/backend/src/entities/`) は手書きせず、稼働中の DB から `sea-orm-cli` で自動生成する。
- マイグレーション crate は `backend` に依存しない独立 crate なので、**backend がコンパイルエラーでも `sea-orm-cli migrate` が動く**（鶏卵問題の回避）。

初期スキーマは単一ファイル `migration/src/m20260520000000_initial_schema.rs` に全テーブル・インデックス・制約を集約している。以降の変更は同ファイルを編集するか、新しい増分マイグレーションを追加する。

---

## 2. マイグレーションの実行

`apps/backend/migration` ディレクトリで実行する（`apps/backend/.env` の `database_url` を参照）。

```sh
# 未適用のマイグレーションを全適用
sea-orm-cli migrate up

# 全テーブルを DROP してから再適用（開発 DB 専用）
sea-orm-cli migrate fresh

# 直近のマイグレーションをロールバック
sea-orm-cli migrate down

# 適用状況を確認
sea-orm-cli migrate status
```

> [!WARNING]
> `migrate fresh` / `refresh` / `reset` は **全テーブルを DROP する破壊操作**。本番・共有 DB には絶対に流さず、必ず捨て DB に向けて実行すること。

---

## 3. エンティティの再生成

スキーマを変更したら、対象テーブルのエンティティを再生成する。

```sh
# カンマ区切りでテーブル名を渡す
scripts/seaorm_generate.sh tasks,sprints
```

このスクリプトは内部で次を行う:

1. `sea-orm-cli generate entity` で `apps/backend/src/entities/_generated/` に純粋な生成物を出力
2. `scripts/seaorm_postprocess.sh` を実行して、生成物に**型付けを再適用**

`database_url` は `apps/backend/.env` から読み込む。

### postprocess が再適用する内容

`sea-orm-cli` は varchar 列を `String`、json 列を `Json` として吐くため、ドメイン型に戻す処理を後追いで当てている（冪等。再実行は no-op）。

| テーブル | 列 | 付け直す型 |
|----------|----|-----------|
| `tasks` | `priority` | `TaskPriority` |
| `project_custom_fields` | `field_type` | `CustomFieldType` |
| `sprints` | `status` | `SprintStatus` |
| `project_members` | `role` | `ProjectRole` |
| `drive_files` | `storage_type` | `StorageType` |
| `drive_folder_shares` | `permission` | `SharePermission` |
| `personal_tokens` | `scopes` | `ScopeList` |

加えて:

- `drive_files` / `drive_folder_shares`: 独自の `ActiveModelBehavior`（CHECK 制約バリデーション）を持つため、生成された既定実装を削除して impl 衝突を回避する。
- `sprints.project_id`: `sea-orm-cli` が誤検出する単一列 unique を除去する。実際の制約は partial unique `idx_sprints_active_per_project ON sprints(project_id) WHERE status = 'active'` で、単純 unique 化すると 1 プロジェクト 1 スプリントしか作れなくなるため。

> 新しく varchar-backed enum 列や JSON ドメイン型列を追加した場合は、`scripts/seaorm_postprocess.sh` に対応する `replace` 行を追記すること。追記しないと再生成のたびに `String` / `Json` へ戻ってしまう。

---

## 4. スキーマ変更の手順

1. `migration/src/m20260520000000_initial_schema.rs` を編集（または増分マイグレーションを追加）。CHECK 制約・UNIQUE・インデックス・FK の `ON DELETE` まで漏れなく記述する。
2. 捨て DB に対して `sea-orm-cli migrate fresh` で適用し、SQL が通ることを確認。
3. `scripts/seaorm_generate.sh <変更したテーブル>` でエンティティを再生成。
4. `_generated/` の差分と、必要なら `scripts/seaorm_postprocess.sh` のマッピングを確認。
5. `cargo check`（backend）でエンティティとアプリコードの整合を確認。

---

## 5. 全文検索インデックス (USE_PG_BIGM)

`tasks` の全文検索インデックスは環境変数 `USE_PG_BIGM` で切り替わる。

- `USE_PG_BIGM=true`: `pg_bigm` の trigram GIN インデックス（`idx_tasks_title_bigm` / `idx_tasks_description_bigm`）を作成し、`search_vector` 生成列は作らない。
- それ以外（既定）: 生成列 `search_vector`（`tsvector`）+ `idx_tasks_search_vector`（GIN）を作成する。

`migrate fresh` 実行時の `USE_PG_BIGM` の値で、生成されるスキーマが変わる点に注意。

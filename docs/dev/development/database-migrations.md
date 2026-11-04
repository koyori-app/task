---
title: マイグレーション
description: SeaORM エンティティをスキーマファーストで運用するためのマイグレーション手順とエンティティ再生成の流れ
updated: 2026-11-04
sidebar:
  order: 1
---

バックエンドの DB スキーマとエンティティはスキーマファーストで運用する。

- スキーマの定義元は `apps/backend/migration` crate（SQL マイグレーション）である。
- エンティティ（`apps/backend/crates/entity/src/_generated/`）は手書きせず、稼働中の DB から `sea-orm-cli` で生成する。
- マイグレーション crate は backend のワークスペースから除外した独立 crate である（`apps/backend/Cargo.toml` の `exclude`）。そのため、backend がコンパイルエラーでもマイグレーションを実行できる。

初期スキーマは `migration/src/m20260520000000_initial_schema.rs` にまとまっており、以降の変更は増分マイグレーションとして同じディレクトリに並ぶ。

## マイグレーションの実行

`apps/backend/migration` で実行する。接続先は環境変数 `DATABASE_URL` で渡す。`apps/backend/.env` の `database_url`（小文字）は backend と再生成スクリプトが読むもので、この CLI には渡らない。

```sh
cd apps/backend/migration
export DATABASE_URL=postgresql://task:task@localhost:5432/task

# 未適用のマイグレーションを全適用
cargo run -- up

# 全テーブルを DROP してから再適用（開発 DB 専用）
cargo run -- fresh

# 直近のマイグレーションをロールバック
cargo run -- down

# 適用状況を確認
cargo run -- status
```

Docker Compose では `migration` サービスが `/app/run-migration.sh` を実行し、`up` と[起動時の schema sync と UNIQUE インデックス](#起動時の-schema-sync-と-unique-インデックス)の節のインデックス削除を行う。

> [!WARNING]
> `fresh` は全テーブルを DROP し、`refresh` と `reset` は全マイグレーションをロールバックする。どれもデータを失う操作なので、本番や共有の DB には流さず、捨ててよい DB に向けて実行する。
> Compose の DB で `fresh` を使うと apalis の適用記録（`public._sqlx_migrations`）も消え、backend が `42P06`（schema "apalis" already exists）で起動できなくなることがある。Compose で初期化するときは `docker compose run --rm migration /app/migration fresh` を手動で実行する。

## エンティティの再生成

スキーマを変更したら、対象テーブルのエンティティを再生成する。

```sh
# カンマ区切りでテーブル名を渡す
scripts/seaorm_generate.sh tasks,sprints
```

このスクリプトは内部で次の二つを行う。

1. `sea-orm-cli generate entity` で `apps/backend/crates/entity/src/_generated/` に生成物を出力する
2. `scripts/seaorm_postprocess.sh` を実行して、生成物に型を付け直す

接続先はスクリプトが `apps/backend/.env` の `database_url` から読み込む。

### postprocess が再適用する内容

`sea-orm-cli` は varchar 列を `String`、json 列を `Json` として出力するので、ドメイン型へ戻す処理を後から当てる。処理は冪等で、再実行しても変わらない。

| テーブル | 列 | 付け直す型 |
|----------|----|-----------|
| `tasks` | `priority` | `TaskPriority` |
| `project_custom_fields` | `field_type` | `CustomFieldType` |
| `sprints` | `status` | `SprintStatus` |
| `project_members` | `role` | `ProjectRole` |
| `tenant_members` | `role` | `TenantRole` |
| `drive_files` | `storage_type` | `StorageType` |
| `drive_folder_shares` | `permission` | `SharePermission` |
| `personal_tokens` | `scopes` | `ScopeList` |

型の付け直しのほかに、次の二つの修正も行う。

- `drive_files` / `drive_folder_shares` は独自の `ActiveModelBehavior`（CHECK 制約の検証）を持つので、生成された既定の実装を削除して impl の衝突を避ける。
- `sprints.project_id` は、`sea-orm-cli` が誤検出する単一列 unique を除去する。単一列の unique にすると、1 プロジェクトに 1 スプリントしか作れなくなるためである。

varchar の enum 列や JSON のドメイン型列を新しく追加したら、`scripts/seaorm_postprocess.sh` に対応する `replace` 行を追記する。追記しないと、再生成のたびに `String` / `Json` へ戻る。

## スキーマ変更の手順

1. `apps/backend/migration/src/` に増分マイグレーションを追加する。CHECK 制約、UNIQUE、インデックス、FK の `ON DELETE` まで漏れなく書く。適用済みのマイグレーションは `up` で再実行されないので、初期スキーマを編集しても既存の DB には反映されない。
2. ファイル名は `m<YYYYMMDDhhmmss>_<name>.rs` とする。時刻は既存の最大値より後ろで、main と他のオープン PR のマイグレーションと重ならない値にする。`lib.rs` の `mod` と `Box::new(...)` は時刻順の末尾に足す。
3. 捨ててよい DB で `cargo run -- fresh` を実行し、SQL が通ることを確認する。
4. `scripts/seaorm_generate.sh <変更したテーブル>` でエンティティを再生成する。
5. `_generated/` の差分と、必要なら `scripts/seaorm_postprocess.sh` のマッピングを確認する。
6. `cargo check --workspace --all-targets`（`apps/backend`）で、エンティティとアプリケーションコードの整合を確認する。

## 全文検索インデックス（USE_PG_BIGM）

`tasks` の全文検索インデックスは環境変数 `USE_PG_BIGM` で切り替わる。値が `1`、`true`、`True`、`TRUE` のいずれかなら有効である。

- 有効なときは `pg_bigm` の GIN インデックス（`idx_tasks_title_bigm` / `idx_tasks_description_bigm`）を作成し、`search_vector` 生成列は作らない。マイグレーションは `pg_bigm` の `CREATE EXTENSION` を行わないので、拡張は先に DB へ入れておく。
- それ以外（既定）は、生成列 `search_vector`（`tsvector`）と `idx_tasks_search_vector`（GIN）を作成する。

初期スキーマの適用時の値で生成されるスキーマが変わるので、`fresh` の実行時には `USE_PG_BIGM` の値に注意する。検索ハンドラーも同じ環境変数で検索方法を切り替えるので、マイグレーションと backend で値を揃える。

これとは別に、`m20261020000000_tasks_title_trgm` は `USE_PG_BIGM` に関係なく `pg_trgm` と `idx_tasks_title_trgm` を作成する。DB の `lc_ctype` が `C` だと日本語の trigram が作られないので、このマイグレーションはエラーで止まる。UTF-8 のロケールで作った DB を使う。

## 起動時の schema sync と UNIQUE インデックス

backend は起動時に SeaORM の schema sync（`get_schema_registry("entity::*").sync`）を無条件に実行する。sync は、エンティティの `unique` / `unique_key` と列の組が一致しない UNIQUE インデックスを `DROP CONSTRAINT` で消そうとする。実体がインデックスだと Postgres が `42704` を返し、backend は起動しない。

この衝突を避けるため、マイグレーション適用後、sync の前に次の四つのインデックスを削除する。

- `projects_key_tenant_unique`
- `labels_project_name_unique`
- `idx_sprints_active_per_project`
- `idx_projects_personal_owner`

削除は `apps/backend/docker/run-migration.sh`、`e2e/scripts/start-backend.sh`、統合テストのハーネスのそれぞれで行う。ハーネスは後ろ二つだけを削除し、`idx_sprints_active_per_project` は sync の後に作り直す。

`idx_sprints_active_per_project`（`sprints(project_id) WHERE status = 'active'`）は、エンティティに対応する `unique_key` が無いので sync でも作られない。したがって Compose で動かす環境にこの部分 UNIQUE インデックスは残らず、「プロジェクトで有効なスプリントは一つ」という規則はスプリント開始のハンドラーの確認だけで守っている。

これらの理由から、部分 UNIQUE インデックス（`CREATE UNIQUE INDEX ... WHERE`）をマイグレーションに足さない。NULL を除外したいだけなら列の UNIQUE 制約で足りる（NULL 同士は重複扱いされない）ので、エンティティにも `#[sea_orm(unique)]` を付けて揃える。

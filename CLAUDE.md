# CLAUDE.md

前半はこのプロジェクト固有の知見、後半は汎用の行動指針。**固有知見が優先。**

## プロジェクト構成

- `apps/backend` — Rust (axum + SeaORM + apalis)。Cargo ワークスペース
- `apps/frontend` — Vike + React。`openapi.json` から API 型を生成
- `apps/cli` — TypeScript CLI

### backend ワークスペース（依存は一方向・逆流禁止）

```
entity → common → payload → service → job → handler → backend(bin)
```

| クレート | 置くもの |
|---|---|
| `entity` | SeaORM エンティティ（sea-orm-cli 生成物。手で整形しない） |
| `common` | error / settings / db ヘルパー / 通知定数など最下層の横断コード |
| `payload` | リクエスト/レスポンス DTO。**依存は entity + common のみに閉じる** |
| `service` | ビジネスロジック・横断サービス（旧 `src/utils`） |
| `job` | apalis ジョブ。ワーカーは `AppState` ではなく `JobState` を受け取る |
| `handler` | axum ハンドラー / extractors / routes / openapi / middlewares / `AppState` |
| `backend` | `main` / `server` / `export_openapi` の glue のみ |

- 新しい DTO は payload、ロジックは service へ。ハンドラー間で共有したい処理も service に降ろす
- `backend::handlers` 等の再エクスポートは統合テスト互換のためのもの。新規コードは各クレートを直接 use する

## 検証（backend、コミット前に必ず）

```bash
cargo fmt                              # 忘れると fmt CI で落ちる
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets
cargo test --workspace --lib
```

- 統合テスト（`tests/`）は実 Postgres / Redis 必須（`.github/workflows/backend-test.yml` が正）。ローカルに無ければ CI で確認し、PR 本文にその旨を書く
  1. CI と同じサービスを起動（`--rm` 付き。停止すれば自動でコンテナも消える）:
     ```bash
     docker run -d --rm --name koyori-test-pg -e POSTGRES_USER=test -e POSTGRES_PASSWORD=test -e POSTGRES_DB=test -p 5432:5432 postgres:17
     docker run -d --rm --name koyori-test-redis -p 6379:6379 valkey/valkey:8.1
     ```
  2. `apps/backend/.env`（gitignore 済み）に backend-test.yml の `env:` ブロックと同じ値を置く（ハーネスの `load_dotenv()` が読む）。GitHub App 系は設定不要（`load_github_test_env()` が自前注入）。SMTP は実サーバー不要
  3. CI に合わせてシングルスレッドで実行: `cargo test -- --test-threads=1`（CI は `cargo nextest run --test-threads=1`）
  4. 終わったら必ずコンテナを停止・削除する（上げっぱなし禁止）: `docker stop koyori-test-pg koyori-test-redis`
- API 表面を変えたら: `cd apps/frontend && pnpm openapi && node_modules/.bin/vp fmt`
  - 整形は **`vp fmt`**（prettier は入っていない）。`api.d.ts` は gitignore 済み
  - API を変えていない PR では `openapi.json` の差分ゼロが検証項目になる

## 地雷（実際に踏まれた・発見されたもの）

- **SeaORM の生 SQL に `?` プレースホルダを書かない。** `Statement::from_sql_and_values` は SQL を無変換で sqlx に渡すため、Postgres では実行時構文エラーになる。`common::db` のヘルパー（`table_exists` / `column_exists` / `execute_bound` / `query_one_bool`）か `$N` 直書きを使う。この類のバグは過去に3箇所で見つかっている（#272 / #277）
- **`#[utoipa::path]` の path は nest 位置からの相対パス。** routes 側で同じパスを `.nest()` すると二重連結された URL に登録されて 404 になる（#277 で実発生）。既存ハンドラーの登録方法に合わせること
- **apalis のジョブペイロードは Postgres（apalis.jobs）に平文で永続化される。** トークン等の機微情報を載せない（Redis のみに保持する）。job クレートの「シリアライズ後キー集合」固定テストが回帰ガード。再送競合は `issued_at` 世代（Unix ミリ秒）を process 時に生成し、`email_verification::store_token` の世代比較（Lua）で後勝ち解決する
- **ワーカーに `AppState` を渡さない**（job → handler の循環になる）。必要な依存は `JobState` にフィールドを足す
- 増分ビルドの計測に `cargo build -p <crate>` を使わない。feature 解決がワークスペース全体と変わり依存を作り直すため、数字が実態と乖離する

## テスト・PR の流儀

- バグ修正 PR には**修正前の main で fail する回帰テスト**を付ける（バグの証明として機能させる）
- 統合テストは `tests/common` の `TestApp` を使う。拒否系（403/404）と対照の成功系（200/201、過剰拒否でないこと）をセットで書く
- エラーは握り潰さず `?` で伝播する（`unwrap_or(false)` / `let _ =` でのもみ消しが実バグを隠した前例あり）
- コミットは Conventional Commits + 日本語（例: `fix(backend): …` / `refactor(workspace): …`）。1 Phase・1 関心 = 1 PR
- PR 本文も日本語。「概要 / 変更内容 / 挙動の変化 / テスト」の構成

## 行動指針（汎用）

**注意深さ優先のバイアスがある。些末なタスクでは常識で判断。**

1. **考えてから書く** — 前提を明示する。解釈が複数あれば選ばずに提示する。不明点は止まって聞く。より単純な方法があるなら言う
2. **シンプル第一** — 頼まれていない機能・単一用途への抽象化・「将来のための柔軟性」を書かない。200行が50行で済むなら書き直す
3. **外科的変更** — 触るのは必要な箇所だけ。隣のコードの「ついで改善」をしない。既存スタイルに合わせる。自分の変更で不要になった import 等は消し、無関係な死にコードは報告に留める
4. **ゴール駆動** — タスクを検証可能な形に変換する（「バグ修正」→「再現テストを書いて通す」）。各ステップに検証を紐付け、通るまでループする

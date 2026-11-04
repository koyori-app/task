---
title: タスクの類似検索
description: pg_trgm による title の trigram 類似度で、曖昧検索と類似タスク候補を実現する設計
updated: 2026-11-04
sidebar:
  order: 3
---

## 目的

次の 3 つを、PostgreSQL 同梱の `pg_trgm` による title の trigram 類似度 1 本で実現する。API の形と現在の振る舞いは[タスク拡張](/dev/features/tasks/extensions)に書き、このページは設計の理由と比較を残す。

| # | 機能 | 入口 |
|---|---|---|
| ① | 曖昧検索（部分一致・表記ゆれ） | `GET /tasks/search`（ダッシュボード検索、親タスク・サブタスクの選択 UI） |
| ② | 関連タスク候補 | タスク詳細に「似ているタスク」欄 |
| ③ | 作成時の重複候補 | 作成ダイアログ（`CreateTaskDialog.vue`）のタイトル入力中に候補を表示 |

検索エンジン（Meilisearch 等）は入れない。別のデータソース・同期ジョブ・認可の再検証・コンテナ運用が増える一方、この 3 機能は trigram で足りる。横断検索（コメント・ドライブ・PR）や意味的な類似が必要になった時点で再検討する。

## 背景

導入前の `search_tasks`（`handler/src/handlers/task_extensions.rs`）は既定で `search_vector`（`to_tsvector('pg_catalog.simple', title || description)`）を `plainto_tsquery` で引いていた。問題は次のとおりである。

- `simple` 設定は空白・記号でしか語を切らないため、日本語は「ログイン画面の修正」が 1 トークンになる。「ログイン」で検索してもヒットしない
- 英語もトークン完全一致のみである。前方一致（`auth` から `authentication`）や typo に弱い
- 日本語向けの `USE_PG_BIGM=true` 分岐は、本番の `postgres:17` イメージに pg_bigm が無いので使えない

`pg_trgm` は contrib 同梱で `postgres:17` にそのまま入っており、PG13 以降は trusted extension なので DB への `CREATE` 権限で `CREATE EXTENSION` できる（superuser 不要）。テストハーネス（testcontainers の `postgres:17`）も同じである。

### 本番での確認

2026-10-20 に dokploy 上の `postgres:17.10` を読み取りのみで確認した結果は次のとおりである。

| 項目 | 値 |
|---|---|
| DB ロケール / encoding | `en_US.utf8` / `UTF8`（CJK が trigram 化される条件を満たす） |
| `pg_trgm` | 利用可能（1.6）、未インストール。入っている拡張は `plpgsql` と `pgcrypto` のみ |
| 接続ロール `task` | superuser かつ DB owner。`CREATE EXTENSION` に支障なし |
| `USE_PG_BIGM` | 未設定（tsvector 経路で稼働中）。pg_bigm は利用不可 |
| `tasks` | 478 行（生存 463、日本語タイトル 456）、9 プロジェクト、テーブル全体 4.3 MB、title 平均 32 文字・最大 114 文字 |
| 既存 index | `idx_tasks_search_vector`（GIN）のみ |

当時の検索の取りこぼしを本番データで比較した。`search_vector @@ plainto_tsquery` と `title/description ILIKE` のヒット数は次のとおりである。

| 検索語 | tsvector | ILIKE |
|---|---|---|
| 修正 | 1 | 12 |
| 追加 | 9 | 116 |
| タスク | 4 | 115 |
| レビュー | 3 | 59 |
| 対応 | 25 | 55 |
| GitHub | 44 | 66 |
| PR | 74 | 158 |

日本語の一般語で 1 桁〜10 倍以上の差があり、[背景](#背景)の問題は実データでも確認できた。

## 全体像

経路は次のとおりである。

```text
 ① 検索ボックス              ③ 作成ダイアログ            ② タスク詳細
   q                           title（300ms debounce）     title, exclude_task_id=自分
   │                           │                           │
   ▼                           └──────────┬────────────────┘
 GET /tasks/search                          ▼
   WHERE に title/description ILIKE     GET /tasks/similar
   %q% と q <% title を OR                  │
   ORDER BY title 一致 → score              ▼
   │                            service::tasks::find_similar()
   │                              title % $text（similarity 降順）
   └──────────────┬───────────────────────┘
                  ▼
   tasks.title + GIN (title gin_trgm_ops)   ← migration で pg_trgm 拡張と index を追加
```

①は「検索語がタイトルに含まれるか」（`ILIKE`、部分一致の保証）と「近似的に含まれるか」（`<%`、表記ゆれ）、②③は「タイトル同士が全体として似ているか」（`%`）で、演算子を使い分ける。いずれも GIN `gin_trgm_ops` が効く。

`<%`（`word_similarity`）だけでは部分一致を保証できない。検索語には語境界の空白 trigram が付くため、「ログイン」と「認証ログイン画面」のように日本語の連続文字列の途中にある場合は共有 trigram が減り、既定の 0.6 を下回る。部分一致は `ILIKE` が担う。

## DB

### マイグレーション

[m20261020000000_tasks_title_trgm.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261020000000_tasks_title_trgm.rs) が `pg_trgm` 拡張（`IF NOT EXISTS`）と GIN index `idx_tasks_title_trgm` を足す。判断は次のとおりである。

- 対象は title のみである。description は index が大きくなる割に、重複判定・候補提示には title で足りる。description の全文検索は `search_vector` を残す
- UNIQUE でない通常の index なので、起動時の SeaORM `sync()`（2.0.4）に消されない。AGENTS.md の部分 UNIQUE index の既知の落とし穴には該当しない。`sync()` は拡張も index も作らないので、migration の適用漏れは補えない。migration の適用自体は e2e CI の起動（`e2e/scripts/start-backend.sh` が実 migration バイナリで適用してから backend を起動）で踏まれ、失敗すれば全ケースが落ちる
- `down` は index だけ落とす。拡張は `IF NOT EXISTS` で既存を流用した可能性があり、他の依存や別ロール所有だと `DROP EXTENSION` が失敗するため残す
- 通常の `CREATE INDEX` は構築中 `tasks` への書き込みを待たせる。本番 compose の `depends_on` は新しい backend の起動を migration 完了まで待たせるだけで、更新デプロイ中は旧 backend とワーカーが動いたまま index が構築される。構築中の書き込み待ちは許容する（title のみの GIN で、本番は 478 行・4.3 MB なので 1 秒未満）。`CONCURRENTLY` は `MigrationTrait::use_transaction()` を `Some(false)` にすれば使えるが、失敗時の INVALID index の片付けが要るので、待ちが問題になる規模になるまで単純な `CREATE INDEX` を採る
- ロケールを検査する。trigram は `lc_ctype` が `C` だと CJK を英数字とみなさず trigram を作らない。マイグレーション内で `SELECT array_length(show_trgm('日本語テスト'), 1)` を実行し、`NULL` か 0 ならエラーで止める。本番は `en_US.utf8` と確認済みで、検査は他環境（セルフホスト、手元の `TEST_DATABASE_URL`）向けに残す

### テストハーネス

`tests/common/mod.rs` の `ensure_test_env()` は `sync()` で schema を作り、マイグレーションは流さない（`search_vector` も手で足している）。同じ場所で `pg_trgm` 拡張と `idx_tasks_title_trgm` を `IF NOT EXISTS` で作る。`TEST_DATABASE_URL` で手元の Postgres を使う場合は、その DB に `CREATE` 権限が要る。

### しきい値

`%` は `pg_trgm.similarity_threshold`（既定 0.3）、`<%` は `pg_trgm.word_similarity_threshold`（既定 0.6）を使う。既定値で始めた。日本語の短いタイトル（trigram が少ない）で取りこぼすなら、`SET LOCAL` をクエリと同じトランザクションで発行して下げる。GUC をサーバー側で恒久変更すると、設定が見えない場所に散るので行わない。

## service

`service::tasks::find_similar`（`crates/service/src/tasks.rs`）が②③の両方を支える。引数は `project_id`、trim 済みで非空の `text`、`exclude_task_id`、`limit` である。戻りは `SimilarTask`（`id`、`seq_id`、`title`、`score`）の並びである。

同じプロジェクトの削除されていない（`deleted_at IS NULL`）タスクのうち、`title % text` のものを `similarity(title, text)` の降順で返す。`exclude_task_id` があれば除く。`score` の同値で順序が揺れないよう、`search_tasks` と同じく id で固定する。プレースホルダは `$N` で書く（`?` を書かない。AGENTS.md の既知の落とし穴）。

## API

### ① `GET /tasks/search`

`search_tasks_tsvector` の WHERE は、`search_vector @@ tsq`、`title ILIKE`、`COALESCE(description, '') ILIKE`、`q <% title` の OR である。count と一覧の両方に同じ条件を使う。ずれると `total` が合わない。ILIKE のパターンは `escape_like` 済みの `%q%` である。

- `tsq` は `FROM tasks CROSS JOIN plainto_tsquery('pg_catalog.simple', q) AS tsq` で 1 回だけ組む。ORDER BY で SELECT の別名は式の中では使えないためである
- description の ILIKE は index なし（trigram index は title のみ）だが、`project_id` で絞った後の行数なら実用上足りる。これが無いと description にだけ「ログイン画面が崩れる」と書かれたタスクが `q=ログイン` でヒットせず、[背景](#背景)の問題が title でしか直らない。`search_tasks_ilike`（pg_bigm 経路）と同じ形である
- 並び順はタイトル一致（`title ILIKE` または `q <% title`）を第 1 キー、`GREATEST(ts_rank(search_vector, tsq), word_similarity(q, title))` を第 2 キー、id を第 3 キーにする。`ts_rank`（重み D=0.1、通常 0.1 前後）と `word_similarity`（0.6〜1.0）は値域が揃わないので、大小で比べない
- `score` には第 2 キーの値を返す。ILIKE のみで一致した行（2 文字の日本語など）は `score` が 0 になり得る
- レスポンス形（`SearchTasksResponse`）は変えていない
- `highlight_ilike` の `find_match_char_range` は、検索語の方が対象文字列より長ければ `None` を返す。修正前は範囲外スライスで panic していた（`saturating_sub` が 0 になっても 1 回ループする）。検索語は `trim()` しかしないため `q="a      b"`（内部の空白連続、8 文字）が title `a b`・description なし（text は 4 文字）に `'a' & 'b'` で一致して panic し、`CatchPanic` 層が無いので接続が切れた。`<%` の近似一致（`authenticationxx` から `authentication`）でも到達するので、PR 1 で既存バグの修正として直した。近似一致で部分文字列でない場合は `<em>` なしで返す
- `USE_PG_BIGM=true` の `search_tasks_ilike` 経路は触らない（ILIKE で既に部分一致する）。分岐の撤去は本件の範囲外である

### ②③ `GET /tasks/similar`

呼び方は次のとおりである。

```text
GET /v1/tenants/{tenant_id}/projects/{project_id}/tasks/similar?title=ログイン画面&exclude_task_id=<uuid>&limit=5
```

| パラメータ | 必須 | 既定 | 制約 |
|---|---|---|---|
| `title` | ○ | | trim 後に空、または 255 文字（文字数。`CreateTaskRequest` と同じ）を超えたら 400。長い入力の trigram 比較コストを抑える |
| `exclude_task_id` | | なし | 自タスクを除く（②用） |
| `limit` | | 5 | 上限 20（超過は 20 に丸める。`search` の `min(100)` と同じ流儀） |

レスポンスの例は次のとおりである。

```json
{ "tasks": [ { "id": "uuid", "seq_id": 42, "title": "ログイン画面の修正", "score": 0.58 } ] }
```

- 認可は `search_tasks` と同じで、`Scope::ReadTask` + `ensure_tenant_access(tenant_id, Some(project_id))` である。候補は同一プロジェクト内に限る（`project_id` で WHERE）。テナント内の他プロジェクトは出さない（権限表が増えるのを避ける。必要になったら別途）
- DTO は `payload::task_extensions` の `SimilarTasksQuery` / `SimilarTaskHit` / `SimilarTasksResponse` である。レスポンス DTO には `Deserialize` も付ける（CLI 規約）。スキーマは `routes!` から自動収集されるので手登録は不要である
- routes は `tenants.rs` の `/{project_id}/tasks` nest に `search_tasks` と並べて登録する（`#[utoipa::path]` は nest からの相対パス `/similar`）。`/{task_id}` より静的セグメントが優先されるので順序は問題にならない
- `GET /tasks/{task_id}/similar` は作らない。③の時点ではタスク id が無く、1 本で両方を賄える

## フロントエンド

composable `useSimilarTasks({ tenantId, projectId, title, excludeTaskId })` を共通で使い、`title` が trim 後 2 文字未満なら `enabled: false` にする。型は `openapi.json` の再生成（`pnpm openapi` と `vp fmt`）で付く。画面ごとの扱いは次のとおりである。

| 画面 | 扱い |
|---|---|
| ③ `CreateTaskDialog.vue` | タイトル入力を 300ms debounce（`tasks/+Page.vue` の `SEARCH_DEBOUNCE_MS` と同じ手法）して問い合わせる。タイトル欄の下に最大 5 件を表示する。作成はブロックしない（注意喚起のみ。取得失敗時も作成できる）。`TaskTitleLink` と `taskDetailHref` は表示用の tenant ID を使い、ダイアログは API 用 UUID の `tenantId` しか持たないので、ダイアログ内で `usePageContext().routeParams.tenant` を読む（他の component 層でも使っている手法。これで `DashboardCreateTask.vue` / `HomeDashboard.vue` の変更が不要）。`TaskTitleLink` は通常クリックで遷移せず `select` を emit するので、ダイアログ側で受けて `navigate(taskDetailHref(tenantDisplayId, projectKey, seqId))` し、ダイアログを閉じる |
| ② タスク詳細 | 「似ているタスク」欄である。`TaskDetailHub.vue` は tenant id を受け取っていないため、Hub 内ではなく呼び出し側が既存の `main` slot に置く。`tasks/@taskId/+Page.vue` では `select` で `navigate`、`TaskDetailPane.vue` では既存の `open-task-ref` emit に繋ぐ（ペイン内でタスクを切り替える既存経路）。形は `{ projectKey, seqKey, href }` なので、候補の `seq_id` から `taskSeqKey()` と `taskDetailHref()` で組み立てる（`open-task` は `TaskResponse` 全体を要求するので候補では使えない）。`title` に現在のタイトル、`excludeTaskId` に自分の id を渡す。0 件なら欄ごと非表示にする |
| ① | 変更なし（同じエンドポイント） |

古い応答は見せない。TanStack Query は `enabled: false` にしてもキャッシュ済みの `data` を返すので、無効化では消えない。query key に tenant / project / title / excludeTaskId を全部含め（key が変われば `data` は新しい key のものになる）、遅れて届いた旧応答は旧 key に保存されて表示されない。残る隙間は debounce 中の 300ms だけなので、表示条件は「ダイアログが開いている ∧ `debouncedTitle === title.trim()` ∧ 2 文字以上」にする。詳細のタスク切替も key が変わるので持ち越さない。

`DashboardCreateTask.vue` は同じ `CreateTaskDialog` を開くので、③の変更がそのまま効く。

## 実装の段階

実装は次の 3 段に分けた。PR 2 と 3 は 1 本にまとめてよいとした。API 単体では利用者が居ないためである。

| PR | 内容 |
|---|---|
| 1 | migration + ハーネス + `search_tasks` の変更 + 回帰テスト（①） |
| 2 | `find_similar` + `GET /tasks/similar` + 統合テスト + `openapi.json` |
| 3 | フロント（③ 作成ダイアログ、② 詳細欄）+ unit テスト |

## 未実装と既知の制限

- 短い語の typo は拾えない（`OAtuh` と `OAuth` は共有 trigram が少ない）。trigram の性質で、主な改善は部分一致と長めの語の表記ゆれである
- 意味的な類似（「ログインできない」と「サインイン失敗」）は対象外である。必要になれば埋め込み + pgvector だが、埋め込み API・pgvector 入りイメージ・更新ジョブが要るため別設計とする
- しきい値の調整は実データで 1 回確認が要る（[しきい値](#しきい値)）
- 性能。GIN は候補抽出までで、類似度順の上位 N 件は候補全件を採点してソートする。短い語・頻出語（「修正」など）で候補が膨らむプロジェクトがあり得る。さらに 2 文字の `ILIKE '%画面%'` は検索側の trigram を抽出できず、GIN の絞り込みが効かない（全 index スキャン + recheck）。本番は 478 行・9 プロジェクトなので当面は問題にならない。「2 文字の語」「頻出語」「他プロジェクトに大量の行がある状態」の 3 ケースで `EXPLAIN ANALYZE` を取り、遅ければ GiST（`<->` の KNN 順序付けが効く）や 3 文字未満の ILIKE 省略を検討する
- ロケール。DB の `lc_ctype` が `C` だと日本語で効かない。本番は `en_US.utf8` を確認済みで、他環境は migration のロケール検査で起動前に検出する

## レビューで見送った指摘

2026-10-20 の設計レビュー（Codex `gpt-6.1-sol`）で挙がったが、採らなかったものを再検討の材料として残す。

| 指摘 | 見送った理由 |
|---|---|
| 「migration、`sync()`、再起動」を通して index と拡張が維持されることを検証するテスト（専用ハーネス、または e2e への 1 ケース追加） | `sync()` が落とすのは UNIQUE index だけで拡張は触らない（sea-orm 2.0.4 `schema/builder.rs`）ことはソースで確認済み。migration の適用は既存 e2e の起動で踏まれ、失敗すれば全ケース落ちる。e2e は現状 users しか seed せず検索 UI までの fixture も無いので 1 ケースでは済まず、index の有無は検索結果に出ないので index の検証にもならない |
| ハーネスでの `show_trgm('日本語テスト')` 非空テスト | ロケールの保証は[マイグレーション](#マイグレーション)内の `show_trgm` 検査が担う。①の日本語部分一致テストでは代わりにならない（`ILIKE` の条件がロケール C でも一致するので、trigram が作られなくても通る）。負のケース（C ロケールで migration が止まる）は手元では試験しない |
| GiST index への変更 | 性能問題が実測で出てから判断する（[未実装と既知の制限](#未実装と既知の制限)） |
| `CREATE INDEX CONCURRENTLY` | sea-orm-migration 2.0.4 では `use_transaction()` を `Some(false)` にすれば実装できる。ただし失敗時に INVALID index が残る片付けが要り、更新デプロイ中に旧 backend が動いたまま構築されても title のみの GIN で現状の件数なら書き込み待ちは秒単位なので、単純な `CREATE INDEX` を採る。待ちが問題になる規模になったら切り替える |
| PAT のテナント・プロジェクト束縛や project-only guest の許可範囲の網羅試験 | `search_tasks` と同じ `ensure_tenant_access` を通すので新しい認可規則は増えない。既存の認可テストが担う範囲を重複して書かない。`ReadTask` 無し PAT の 403 と別テナント project id の拒否だけテストに足した |

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-10-20 | Meilisearch 等の外部検索エンジンは導入しない。pg_trgm で 3 機能を実現する | 別のデータソース・同期ジョブ・認可の再検証・コンテナ運用が増える一方、3 機能は trigram で足りる |
| 2026-10-20 | 類似候補は同一プロジェクト内に限る。テナント横断は行わない | 権限表が増えるのを避ける |
| 2026-10-20 | ②③は `GET /tasks/similar` の 1 本で賄い、`/{task_id}/similar` は作らない | ③の時点ではタスク id が無く、1 本で両方を賄える |
| 2026-10-20 | trigram index は title のみ。description は従来の tsvector のまま | description は index が大きくなる割に、重複判定・候補提示には title で足りる |
| 2026-10-20 | 部分一致の保証は `ILIKE`（title と description、description は index なし）、表記ゆれは `<%` で担う | `<%` だけでは日本語の中間一致を取りこぼす |
| 2026-10-20 | migration の `down` は index だけ落とし、`pg_trgm` 拡張は残す | 拡張は既存を流用した可能性があり、他の依存や別ロール所有だと `DROP EXTENSION` が失敗する |
| 2026-10-20 | 初回 index 構築中の `tasks` への書き込み待ちを許容する。旧 backend の停止は要求しない | title のみの GIN で、本番は 478 行・4.3 MB なので 1 秒未満 |
| 2026-10-20 | 類似候補の UI は `TaskDetailHub` 内ではなく呼び出し側に置く。クリックはページでは遷移、ペインでは `open-task-ref` | `TaskDetailHub.vue` は tenant id を受け取っていない |

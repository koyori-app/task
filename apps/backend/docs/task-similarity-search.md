# タスクの曖昧検索・類似タスク候補（pg_trgm）

> ステータス: **設計中（未実装）**
> 作成日: 2026-10-20

## 1. 目的

次の 3 つを、PostgreSQL 同梱の `pg_trgm` による **title の trigram 類似度** 1 本で実現する。

| # | 機能 | 入口 |
|---|------|------|
| ① | 曖昧検索（部分一致・表記ゆれ） | 既存 `GET /tasks/search`（ダッシュボード検索、親タスク・サブタスクの選択 UI） |
| ② | 関連タスク候補 | タスク詳細（`TaskDetailHub.vue`）に「似ているタスク」欄 |
| ③ | 作成時の重複候補 | 作成ダイアログ（`CreateTaskDialog.vue`）のタイトル入力中に候補を表示 |

検索エンジン（Meilisearch 等）は入れない。別の真実の源・同期ジョブ・認可の再検証・コンテナ運用が増える一方、今回の 3 機能は trigram で足りるため。横断検索（コメント・ドライブ・PR）や意味的な類似が必要になった時点で再検討する。

## 2. 背景: 今の検索の問題

`search_tasks`（`handler/src/handlers/task_extensions.rs`）は既定で `search_vector`（`to_tsvector('pg_catalog.simple', title || description)`）を `plainto_tsquery` で引く。

- `simple` 設定は空白・記号でしか語を切らないため、日本語は「ログイン画面の修正」が 1 トークンになる。**「ログイン」で検索してもヒットしない**
- 英語もトークン完全一致のみ。前方一致（`auth` → `authentication`）や typo に弱い
- 日本語向けの `USE_PG_BIGM=true` 分岐は、本番の `postgres:17` イメージに pg_bigm が無いので使えない。仕様書（`docs/features/tasks/7.extensions.md` §1）は pg_bigm 採用と書いているが実態と合っていない

`pg_trgm` は contrib 同梱で `postgres:17` にそのまま入っており、PG13 以降は trusted extension なので DB への `CREATE` 権限で `CREATE EXTENSION` できる（superuser 不要）。テストハーネス（testcontainers の `postgres:17`）も同じ。

### 本番での確認（2026-10-20、dokploy 上の `postgres:17.10`、読み取りのみ）

| 項目 | 値 |
|---|---|
| DB ロケール / encoding | `en_US.utf8` / `UTF8`（CJK が trigram 化される条件を満たす） |
| `pg_trgm` | 利用可能（1.6）、未インストール。入っている拡張は `plpgsql` と `pgcrypto` のみ |
| 接続ロール `task` | superuser かつ DB owner。`CREATE EXTENSION` に支障なし |
| `USE_PG_BIGM` | 未設定（tsvector 経路で稼働中）。pg_bigm は利用不可 |
| `tasks` | 478 行（生存 463、日本語タイトル 456）、9 プロジェクト、テーブル全体 4.3 MB、title 平均 32 文字・最大 114 文字 |
| 既存 index | `idx_tasks_search_vector`（GIN）のみ |

現行検索の取りこぼしを本番データで比較した（`search_vector @@ plainto_tsquery` と `title/description ILIKE` のヒット数）:

| 検索語 | tsvector | ILIKE |
|---|---|---|
| 修正 | 1 | 12 |
| 追加 | 9 | 116 |
| タスク | 4 | 115 |
| レビュー | 3 | 59 |
| 対応 | 25 | 55 |
| GitHub | 44 | 66 |
| PR | 74 | 158 |

日本語の一般語で 1 桁〜10 倍以上の差があり、§2 の問題は実データでも確認できた。

## 3. 全体像

```
 ① 検索ボックス              ③ 作成ダイアログ            ② タスク詳細
   q                           title（300ms debounce）     title, exclude_task_id=自分
   │                           │                           │
   ▼                           └──────────┬────────────────┘
 GET /tasks/search（既存）                  ▼
   WHERE に title/description ILIKE     GET /tasks/similar（新規）
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

## 4. DB

### マイグレーション

新規マイグレーション（名前は作成時に main と全オープン PR の時刻と重ならない値にする。例 `m20261020000000_tasks_title_trgm`）:

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX idx_tasks_title_trgm ON tasks USING GIN (title gin_trgm_ops);
```

- 対象は **title のみ**。description は index が大きくなる割に、重複判定・候補提示には title で足りる。description の全文検索は従来の `search_vector` を残す
- UNIQUE でない通常の index なので、起動時の SeaORM `sync()`（2.0.4）に消されない（CLAUDE.md の部分 UNIQUE index の地雷には該当しない）。`sync()` は拡張も index も作らないので、migration の適用漏れは補えない。migration の適用自体は e2e CI の起動（`e2e/scripts/start-backend.sh` が実 migration バイナリで適用してから backend を起動）で踏まれ、失敗すれば全ケースが落ちる
- `down` は **index だけ** 落とす。拡張は `IF NOT EXISTS` で既存を流用した可能性があり、他の依存や別ロール所有だと `DROP EXTENSION` が失敗するため残す
- 通常の `CREATE INDEX` は構築中 `tasks` への書き込みを待たせる。本番 compose の `depends_on` は **新しい** backend の起動を migration 完了まで待たせるだけで、更新デプロイ中は旧 backend とワーカーが動いたまま index が構築される。**構築中の書き込み待ちを許容する**（title のみの GIN で、本番は 478 行・4.3 MB なので 1 秒未満）。`CONCURRENTLY` は `MigrationTrait::use_transaction()` を `Some(false)` にすれば使えるが、失敗時の INVALID index の片付けが要るので、待ちが問題になる規模になるまで単純な `CREATE INDEX` を採る
- **ロケール検査**: trigram は `lc_ctype` が `C` だと CJK を英数字とみなさず trigram を作らない。マイグレーション内で `SELECT array_length(show_trgm('日本語テスト'), 1)` を実行し、`NULL` か 0 ならエラーで止める。本番は `en_US.utf8` と確認済み（§2）。検査は他環境（セルフホスト、手元の `TEST_DATABASE_URL`）向けに残す

### テストハーネス

`tests/common/mod.rs` の `ensure_test_env()` は `sync()` で schema を作り、マイグレーションは流さない（`search_vector` も手で足している）。同じ場所に次を追加する。`TEST_DATABASE_URL` で手元の Postgres を使う場合は、その DB に `CREATE` 権限が要る:

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX IF NOT EXISTS idx_tasks_title_trgm ON tasks USING GIN (title gin_trgm_ops);
```

### しきい値

`%` は `pg_trgm.similarity_threshold`（既定 0.3）、`<%` は `pg_trgm.word_similarity_threshold`（既定 0.6）を使う。まず既定値で始める。日本語の短いタイトル（trigram が少ない）で取りこぼすなら、`SET LOCAL` をクエリと同じトランザクションで発行して下げる（GUC をサーバー側で恒久変更しない。設定が見えない場所に散るため）。

## 5. service

`service::tasks` に 1 関数を追加する。②③の両方から使う。

```rust
pub struct SimilarTask { pub id: Uuid, pub seq_id: i32, pub title: String, pub score: f32 }

pub async fn find_similar(
    db: &impl ConnectionTrait,
    project_id: Uuid,
    text: &str,              // trim 済み・非空
    exclude_task_id: Option<Uuid>,
    limit: u64,
) -> Result<Vec<SimilarTask>, DbErr>
```

```sql
SELECT id, seq_id, title, similarity(title, $2)::real AS score
FROM tasks
WHERE project_id = $1
  AND deleted_at IS NULL
  AND ($3::uuid IS NULL OR id <> $3)
  AND title % $2
-- score の同値で順序が揺れないよう id で固定（search_tasks と同じ理由）
ORDER BY score DESC, id ASC
LIMIT $4
```

プレースホルダは `$N`（`?` を書かない。CLAUDE.md 地雷）。

## 6. API

### ① `GET /tasks/search`（変更）

- `search_tasks_tsvector` の WHERE を次に変える（count と一覧の両方。ずれると `total` が合わない。`$3` は `escape_like` 済みの `%q%`）:
  ```sql
  (search_vector @@ tsq
   OR title ILIKE $3 ESCAPE '\'
   OR COALESCE(description, '') ILIKE $3 ESCAPE '\'
   OR $2 <% title)
  ```
  `tsq` は `FROM tasks CROSS JOIN plainto_tsquery('pg_catalog.simple', $2) AS tsq` で 1 回だけ組む（ORDER BY で SELECT の別名は式の中では使えない）。description の ILIKE は index なし（trigram index は title のみ）だが、`project_id` で絞った後の行数なら実用上足りる。これが無いと description にだけ「ログイン画面が崩れる」と書かれたタスクが `q=ログイン` で引き続きヒットせず、§2 の問題が title でしか直らない。`search_tasks_ilike`（pg_bigm 経路）と同じ形
- 並び順はタイトル一致を第 1 キーにする。`ts_rank`（重み D=0.1、通常 0.1 前後）と `word_similarity`（0.6〜1.0）は値域が揃わないので、大小で比べない:
  ```sql
  ORDER BY (title ILIKE $3 ESCAPE '\' OR $2 <% title) DESC,
           GREATEST(ts_rank(search_vector, tsq), word_similarity($2, title)) DESC,
           id ASC
  ```
  `score` には第 2 キーの値を返す。ILIKE のみで一致した行（2 文字の日本語など）は `score` が 0 になり得る
- レスポンス形（`SearchTasksResponse`）は不変
- `highlight_ilike` の `find_match_char_range` は **検索語が対象文字列より長いと範囲外スライスで panic する**（`saturating_sub` が 0 になっても 1 回ループする）。これは **main の既存バグ**で、検索語は `trim()` しかしないため `q="a      b"`（内部の空白連続、8 文字）が title `a b`・description なし（text は 4 文字）に `'a' & 'b'` で一致して panic し、`CatchPanic` 層が無いので接続が切れる。`<%` の近似一致（`authenticationxx` → `authentication`）でも到達するようになるので、PR 1 で「検索語の方が長ければ `None`」を既存バグの修正として入れる。近似一致で部分文字列でない場合は `<em>` なしで返す（従来どおり）
- `USE_PG_BIGM=true` の `search_tasks_ilike` 経路は触らない（ILIKE で既に部分一致する）。分岐の撤去は本件の範囲外

### ②③ `GET /tasks/similar`（新規）

```text
GET /v1/tenants/{tenant_id}/projects/{project_id}/tasks/similar?title=ログイン画面&exclude_task_id=<uuid>&limit=5
```

| パラメータ | 必須 | 既定 | 制約 |
|---|---|---|---|
| `title` | ○ | | trim 後に空、または 255 文字（文字数。`CreateTaskRequest` と同じ）を超えたら 400。長い入力の trigram 比較コストを抑える |
| `exclude_task_id` | | なし | 自タスクを除く（②用） |
| `limit` | | 5 | 上限 20（超過は 20 に丸める。`search` の `min(100)` と同じ流儀） |

レスポンス:

```json
{ "tasks": [ { "id": "uuid", "seq_id": 42, "title": "ログイン画面の修正", "score": 0.58 } ] }
```

- 認可は `search_tasks` と同じ: `Scope::ReadTask` + `ensure_tenant_access(tenant_id, Some(project_id))`。候補は **同一プロジェクト内** に限る（`project_id` で WHERE）。テナント内の他プロジェクトは出さない（権限表が増えるのを避ける。必要になったら別途）
- DTO は `payload::task_extensions` に `SimilarTasksQuery` / `SimilarTaskHit` / `SimilarTasksResponse` を追加。レスポンス DTO には `Deserialize` も付ける（CLI 規約）。スキーマは `routes!` から自動収集されるので手登録は不要
- routes は `tenants.rs` の `/{project_id}/tasks` nest に `search_tasks` と並べて登録する（`#[utoipa::path]` は nest からの相対パス `/similar`）。`/{task_id}` より静的セグメントが優先されるので順序は問題にならない
- `GET /tasks/{task_id}/similar` は作らない。③の時点ではタスク id が無く、1 本で両方を賄えるため

## 7. フロントエンド

共通: composable `useSimilarTasks({ tenantId, projectId, title, excludeTaskId })` を 1 つ作り、`title` が trim 後 2 文字未満なら `enabled: false`。`openapi.json` 再生成（`pnpm openapi` → `vp fmt`）で型が付く。

| 画面 | 変更 |
|---|---|
| ③ `CreateTaskDialog.vue` | タイトル入力を 300ms debounce（`tasks/+Page.vue` の `SEARCH_DEBOUNCE_MS` と同じ手法）して問い合わせ。タイトル欄の下に最大 5 件を `TaskTitleLink` で表示。**作成はブロックしない**（注意喚起のみ。取得失敗時も作成できる）。`TaskTitleLink` と `taskDetailHref` は **表示用の tenant ID** を使い、ダイアログは API 用 UUID の `tenantId` しか持たないので、ダイアログ内で `usePageContext().routeParams.tenant` を読む（他の component 層でも使っている手法。これで `DashboardCreateTask.vue` / `HomeDashboard.vue` の変更が不要）。unit テストが pageContext を provide できなければ props `tenantDisplayId` を足して呼び出し元から渡す。`TaskTitleLink` は通常クリックで遷移せず `select` を emit するので、ダイアログ側で受けて `navigate(taskDetailHref(tenantDisplayId, projectKey, seqId))` し、ダイアログを閉じる |
| ② タスク詳細 | 「似ているタスク」欄。`TaskDetailHub.vue` は tenant id を受け取っていないため、Hub 内ではなく **呼び出し側** が既存の `main` slot に置く。`tasks/@taskId/+Page.vue` では `select` で `navigate`、`TaskDetailPane.vue` では既存の `open-task-ref` emit に繋ぐ（ペイン内でタスクを切り替える既存経路）。形は `{ projectKey, seqKey, href }` なので、候補の `seq_id` から `taskSeqKey()` と `taskDetailHref()` で組み立てる（`open-task` は `TaskResponse` 全体を要求するので候補では使えない）。`title` に現在のタイトル、`excludeTaskId` に自分の id。0 件なら欄ごと非表示 |

**古い応答を見せない**: TanStack Query は `enabled: false` にしてもキャッシュ済みの `data` を返すので、無効化では消えない。query key に tenant / project / title / excludeTaskId を全部含め（key が変われば `data` は新しい key のものになる）、遅れて届いた旧応答は旧 key に保存されて表示されない。残る隙間は debounce 中の 300ms だけなので、表示条件は「ダイアログが開いている ∧ `debouncedTitle === title.trim()` ∧ 2 文字以上」にする。詳細のタスク切替も key が変わるので持ち越さない。
| ① | 変更なし（同じエンドポイント） |

`DashboardCreateTask.vue` は同じ `CreateTaskDialog` を開くので、③の変更がそのまま効く。

## 8. テスト

バグ修正扱いの回帰テストを含む。「修正前の main で fail する」ことを確認する。

- **①**（統合、`tests/integration/`）
  - 日本語の部分一致を先頭・中間・末尾で 1 本ずつ: 「ログイン画面の修正」「認証ログイン画面」「管理者ログイン」が `q=ログイン` でヒットする（**main では 0 件**。これが回帰ガード）。2 文字（`q=画面`）もヒットする
  - `q=authentcation`（typo）で「authentication flow」がヒットする
  - highlight の panic 回帰: `q="a      b"`（空白連続）、title `a b`、description なしで 200 が返る（**main では panic**。これが回帰ガード）。`q=authenticationxx` → title `authentication` も対照として足す（main では 0 件 200 で通るので回帰ガードにはならない）。`find_match_char_range` の単体テストも足す
  - tsvector のみ・ILIKE のみ・`<%` のみ・複数に一致する行を混在させ、`total` が重複計上されず、`limit=2` で 2 ページ以上をめくっても重複・欠落がない
  - 順位: description に検索語が複数回出る行より、title に部分一致する行が上に来る
  - 既存の tsvector のみでヒットするケース（description だけに語がある）が引き続きヒットする。description だけに「ログイン画面が崩れる」がある行も `q=ログイン` でヒットする
- **②③**（統合）
  - 同一プロジェクトの類似タイトルが score 降順で返る
  - `exclude_task_id` のタスクが除かれる。`deleted_at` 付きが除かれる
  - 別プロジェクトのタスクは返らない（同じテナントでも）
  - 非メンバーは 403、メンバーは 200（拒否系と成功系のセット）。別テナントの `project_id` を組み合わせた URL は 404（`verify_project_in_tenant` の既存挙動）。`ReadTask` を持たない PAT は 403
  - `title` が空白のみ → 400。255 文字（日本語、文字数で数える）は 200、256 文字は 400。`limit=21` → 20 件に丸まる（境界越え）
  - 候補が 20 件を超えるとき 20 件で止まり、同点（同じ title）でも順序が安定する
- **フロント**（unit、既存 `CreateTaskDialog.test.ts` に追加）: 入力後に候補が描画される。空入力では問い合わせない。候補クリックで遷移（表示用 tenant ID の URL）してダイアログが閉じる。入力消去の後から旧応答が到着しても表示しない（1 ケース。閉じる・タスク切替は同じ key の仕組みなので重ねない）。取得失敗でも作成ボタンが押せる

## 9. 仕様書の追従（実装 PR で同時に）

- PR 1: `docs/features/tasks/7.extensions.md` §1 の採用エンジンを「`pg_trgm`（標準）。`pg_bigm` は `USE_PG_BIGM` の任意経路」に書き換え、§7 未決事項の「日本語全文検索エンジン」行を決定済みにする
- PR 2: 同 §1 の API 一覧に `/tasks/similar` を追加
- 本書のステータスを「実装済み」にし、`apps/backend/docs/README.md` の索引行を更新

## 10. 段階

| PR | 内容 |
|---|---|
| 1 | migration + ハーネス + `search_tasks` の変更 + 回帰テスト（①） |
| 2 | `find_similar` + `GET /tasks/similar` + 統合テスト + `openapi.json` |
| 3 | フロント（③ 作成ダイアログ、② 詳細欄）+ unit テスト |

PR 2 と 3 は 1 本にまとめてもよい（API 単体では利用者が居ないため）。

## 11. 既知の限界・リスク

- **短い語の typo** は拾えない（`OAtuh` と `OAuth` は共有 trigram が少ない）。trigram の性質で、主な改善は部分一致と長めの語の表記ゆれ
- **意味的な類似**（「ログインできない」と「サインイン失敗」）は対象外。必要になれば埋め込み + pgvector だが、埋め込み API・pgvector 入りイメージ・更新ジョブが要るため別設計
- **しきい値の調整**は実データで 1 回確認が要る（§4）
- **性能**: GIN は候補抽出までで、類似度順の上位 N 件は候補全件を採点してソートする。短い語・頻出語（「修正」など）で候補が膨らむプロジェクトがあり得る。さらに **2 文字の `ILIKE '%画面%'` は検索側の trigram を抽出できず、GIN の絞り込みが効かない**（全 index スキャン + recheck）。本番は 478 行・9 プロジェクトなので当面は問題にならないが、PR 1 で「2 文字の語」「頻出語」「他プロジェクトに大量の行がある状態」の 3 ケースで `EXPLAIN ANALYZE` を取る。遅ければ GiST（`<->` の KNN 順序付けが効く）や 3 文字未満の ILIKE 省略を検討する
- **ロケール**: DB の `lc_ctype` が `C` だと日本語で効かない。本番は `en_US.utf8` を確認済み。他環境は migration のロケール検査で起動前に検出する

## 12. レビューで見送った指摘

2026-10-20 の設計レビュー（Codex `gpt-6.1-sol`）で挙がったが、採らなかったもの。再検討するときの材料として残す。

| 指摘 | 見送った理由 |
|---|---|
| 「migration → `sync()` → 再起動」を通して index と拡張が維持されることを検証するテスト（専用ハーネス、または e2e への 1 ケース追加） | `sync()` が落とすのは UNIQUE index だけで拡張は触らない（sea-orm 2.0.4 `schema/builder.rs`）ことはソースで確認済み。migration の適用は既存 e2e の起動で踏まれ、失敗すれば全ケース落ちる。e2e は現状 users しか seed せず検索 UI までの fixture も無いので 1 ケースでは済まず、index の有無は検索結果に出ないので index の検証にもならない |
| ハーネスでの `show_trgm('日本語テスト')` 非空テスト | §8 ①の日本語部分一致テストがロケール C なら落ちるので二重。負のケース（C ロケールで migration が止まる）は手元では試験しない |
| GiST index への変更 | 性能問題が実測で出てから判断する（§11） |
| `CREATE INDEX CONCURRENTLY` | sea-orm-migration 2.0.4 では `use_transaction()` を `Some(false)` にすれば実装できる。ただし失敗時に INVALID index が残る片付けが要り、更新デプロイ中に旧 backend が動いたまま構築されても title のみの GIN で現状の件数なら書き込み待ちは秒単位なので、単純な `CREATE INDEX` を採る（§4）。待ちが問題になる規模になったら切り替える |
| PAT のテナント・プロジェクト束縛や project-only guest の許可範囲の網羅試験 | `search_tasks` と同じ `ensure_tenant_access` を通すので新しい認可規則は増えない。既存の認可テストが担う範囲を重複して書かない。`ReadTask` 無し PAT の 403 と別テナント project id の拒否だけ §8 に足した |

## 13. 決定事項ログ

| 日付 | 決定 |
|---|---|
| 2026-10-20 | Meilisearch 等の外部検索エンジンは導入しない。pg_trgm で 3 機能を実現する |
| 2026-10-20 | 類似候補は同一プロジェクト内に限る。テナント横断は行わない |
| 2026-10-20 | ②③は `GET /tasks/similar` の 1 本で賄い、`/{task_id}/similar` は作らない |
| 2026-10-20 | trigram index は title のみ。description は従来の tsvector のまま |
| 2026-10-20 | 部分一致の保証は `ILIKE`（title と description、description は index なし）、表記ゆれは `<%` で担う。`<%` だけでは日本語の中間一致を取りこぼすため |
| 2026-10-20 | migration の `down` は index だけ落とし、`pg_trgm` 拡張は残す |
| 2026-10-20 | 初回 index 構築中の `tasks` への書き込み待ちを許容する。旧 backend の停止は要求しない |
| 2026-10-20 | 類似候補の UI は `TaskDetailHub` 内ではなく呼び出し側に置く。クリックはページでは遷移、ペインでは `open-task-ref` |

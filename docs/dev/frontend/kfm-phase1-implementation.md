---
title: KFM の描画
description: task の本文を描画するレンダラ KFM の構成、パイプライン、サニタイズ、キャッシュ、SSR との受け渡し
updated: 2026-11-04
sidebar:
  order: 4
---

KFM (Koyori Flavored Markdown) は、task の本文を描画するレンダラである。Phase 1 の実体は GFM、GitHub alerts、コードブロック着色（starry-night）、サニタイズから成る、GitHub の描画を再現するレンダラ（github profile）である。これに task 固有の拡張として、タスク参照とメンションのチップ、Mermaid の図、コードブロックの枠を加えている。

このページは現在の実装を説明する。設計判断の根拠は別管理の設計書を定義元とする。

## 描画する箇所

本文を描画する箇所は次の五つである。

| 描画する箇所 | 描画する場所 | profile | scope | KEY-N のリンク | メンションのチップ | 本文の上限（文字数） |
|---|---|---|---|---|---|---|
| タスク詳細（`tasks/@taskId/+data.ts` から `TaskDetailHub`） | サーバー | github | `task-<uuid>` | あり | あり | 100,000 |
| 分割ビューとオーバーレイ（`TaskDetailPane`） | サーバー（内部エンドポイント） | github | `task-<uuid>` | あり | あり | 65,536 |
| ドキュメント詳細（`docs/@fileId/+data.ts`、編集プレビューは内部エンドポイント） | サーバー | github | `doc-<file_id>` | あり | なし | 65,536 |
| コメント（`CommentBody`。[コメントとアクティビティ](/dev/features/tasks/collaboration)） | ブラウザ | comment | `comment-<id>` | なし | あり | 65,536 |
| レビュー指摘（`ReviewFindingBody`） | ブラウザ | comment | `finding-<id>` | あり | なし | なし |

`comment` profile は github profile に `remark-breaks` を足したもので、空行を挟まない単一の改行を `<br>` として出す（GitHub のコメント欄の流儀）。

タスク詳細は描画済み HTML と描画元テキスト（`descriptionSource`）を `+Page.vue` 経由で `TaskDetailHub.vue` へ渡し、最新の `task.description` と厳密に一致するときだけ `v-html` する。一致しないときはプレーンテキストで表示する。

## 構成

```
apps/frontend/src/lib/
  remark-gfm/                  GFM 層の薄いラッパと GFM 由来 class の sanitize スキーマ
    index.ts
    content-class.ts           ラッパーのクラス kfm-content の定義元（GFM CSS の scope）
    style.css                  サイドカー CSS（リスト/blockquote/リンク。preflight 対策。
                               全ルール .kfm-content 子孫限定）
  remark-koyori-alerts/        GitHub alerts (> [!NOTE] 等) → callout
    index.ts                   自前 transformer（GitHub alerts の境界規則）
    style.css                  サイドカー CSS（アイコンは名前空間クラス。inline style 不使用）
  rehype-starry-night/         コードブロック着色 (rehype-starry-night の薄いラッパ)
    index.ts                   createRehypeStarryNight（starry-night 実体の共有と失敗回収）
    schema.ts                  pl-* class の sanitize スキーマ
    style.css                  サイドカー CSS（light シート固定と .dark ブリッジ）
  rehype-kfm-code/             コードブロックの枠（帯・行番号・コピーボタン）
    index.ts                   <kfm-code> で包み、data-lang / data-title と行分割を出力する
    element.ts                 コピーボタンを足す custom element
    schema.ts                  kfm-code と pl-line の sanitize スキーマ
    _tag.ts                    タグ名
    style.css                  サイドカー CSS
  remark-kfm-mermaid/          mermaid フェンス → client 描画 custom element
    index.ts                   remark 変換と sanitize スキーマ
    element.ts                 遅延ロード・SVG 描画・再接続処理
    _tag.ts                    タグ名
    style.css                  JS 無効/描画失敗時のソース表示フォールバック
  remark-kfm-raw-html/         生の HTML を文字として出す（html ノード → text ノード）
  remark-kfm-task-ref/         タスク参照 KEY-N → タスク詳細へのリンク
    index.ts                   remark 変換と sanitize スキーマ
  remark-kfm-mention/          メンションのマーカー <@利用者ID> → メンションのチップ
    index.ts                   remark 変換と sanitize スキーマ
    style.css                  サイドカー CSS
  markup-renderer/             KFM コア
    index.ts                   composition root（renderDescription singleton・公開 API）
    _renderer.ts               createRenderer（controlled pipeline・profile memoize）
    _sanitize.ts               DOMPurify 設定（構造専任・registry 方式）
    _schemas.ts                本番と story probe が共用する sanitize スキーマ一覧
    _cache.ts                  L1 キャッシュ（full-text キー）
    _config.ts                 多層 config 解決（Phase 1 はコード既定と system 層）
    _client-registry.ts        カスタム要素の client 登録（kfm-code / kfm-mermaid）
apps/frontend/src/pages/
  +client.ts                   client 専用 entry（カスタム要素登録の呼び出し元）
apps/frontend/server/middlewares/
  render-description.ts        POST /internal/render-description
```

`index.ts` だけが外部 API で、`_*.ts` は内部である。スキーマだけが要る story は `_schemas.ts` を直接使う。コアはプラグインを import せず、composition root が remark 層、rehype 層、sanitize スキーマを注入する。

client registry は例外で、composition root から再エクスポートしない。`+client.ts` は `_client-registry.ts` を直接 import し、サーバー用レンダラ一式が client bundle へ混入するのを防ぐ。この非再エクスポート規約は bundle 境界であり、公開 API の整理ではない。

同じ bundle 境界として、composition root は starry-night を `import()` で遅延ロードする。静的な import や re-export に戻すと common 文法一式を含む約 417.5 KB raw が root の静的グラフへ加わる。`kfm-cache.test.ts` が、副作用 import を含む静的参照の不在と実行時 import を固定している。

## パイプライン

```mermaid
flowchart TD
    A["入力テキスト"] --> B["改行を LF へ正規化"]
    B --> C["remark-parse"]
    C --> D["remark-kfm-raw-html"]
    D --> E["remark-gfm"]
    E --> F["remark-koyori-alerts"]
    F --> G["remark-kfm-mermaid"]
    G --> H["remark-kfm-task-ref"]
    H --> I["remark-kfm-mention"]
    I --> J["remark-breaks（comment profile のみ）"]
    J --> K["remark-rehype"]
    K --> L["rehype-scope-footnote-label（scope 付きのみ）"]
    L --> M["rehype-starry-night"]
    M --> N["rehype-kfm-code"]
    N --> O["rehype-stringify"]
    O --> P["DOMPurify"]
    P --> Q["HTML 文字列を v-html へ渡す"]
```

入力は remark 層（markdown の構文木）、rehype 層（HTML の構文木）、DOMPurify の順に処理される。全 profile で共通であり、違いは `remark-breaks` の有無だけである。

`allowDangerousHtml` は使わない。生の HTML を HTML として通す経路は無く、通してよい生の tag の一覧（allowlist）も無い。DOMPurify の許可は、プラグインが出力する要素と class だけである。プラグインは `data.hName` / `hProperties` の型付きの出力だけを行う。

### 生の HTML

生の HTML は書いたとおりの文字で出す（`remark-kfm-raw-html`）。markdown の構文として読まれた生の HTML は mdast の `html` ノードになり、remark-rehype の既定ではそのまま捨てられて画面から消える。対象は行中の tag、HTML の行で始まる段落、HTML のコメントで、`Vec<String>` の `<String>`、`<b>太字</b>`、`<!-- 注 -->` などが当たる。

そこで全 profile の先頭の remark 層が `html` ノードを同じ値の `text` ノードへ替え、出力で escape する。行中は `text` に繋ぎ、行の HTML は `text` を一つ持つ段落にする。コードスパンとコードブロックの中は元から文字なので変わらない。

要素、属性、scheme が増える経路は無い。文字は `&lt;` に escape され、DOMPurify の入力にも tag として現れない。説明文、コメント、レビュー指摘の本文は同じレンダラを使うので、振る舞いも同じである。

### processor の memoize

processor は profile ごとに 1 回だけ build して memoize する。memoize されるのは scope なし（既定 `clobberPrefix`）の経路だけで、scope 付き描画は毎回 build する。scope の値（comment id など）には上限が無く、singleton に溜めるとメモリが漏れるためである（`_renderer.ts` の `getProcessor` の分岐）。

build のたびに高い初期化が走るわけではない。starry-night の WASM 読み込みと文法登録はプラグイン factory の closure で共有しており、構築回数に比例しない。

### 既定の profile

描画の既定 profile は composition root が `CreateRendererOptions.defaultProfile` へ `contentConfig.defaultProfile` を渡して決める。未指定時の fallback は `github` である。

## GitHub alerts（remark-koyori-alerts）

GitHub 互換の境界仕様をテストで固定している。

- 5 種のみ（NOTE / TIP / IMPORTANT / WARNING / CAUTION）で、type は大文字小文字を区別しない
- マーカーは blockquote の先頭行に単独で置く。同じ行に後続テキストがあれば通常の blockquote になる。行末スペース 2 つの hard break は単独として扱う
- ネストは不可である。alert 内の alert と、通常の blockquote 内の alert のどちらも、内側は通常の blockquote のままにする。不正な type も通常の blockquote になる
- エスケープした `\[!NOTE]` はマーカーではない（原文のマーカー行と照合する）
- 出力は `div.kfm-alert.kfm-alert--{type}` と `p.kfm-alert__title` で、inline style は出さない
- アイコンと配色は `style.css` の名前空間クラスで当てる（呼び出し側で明示的に import するサイドカー方式）

## タスク参照の自動リンク（remark-kfm-task-ref）

マッチは `([A-Z][A-Z0-9]{1,9})-(\d+)` である。前が英数字、`_`、`/`、`-`、後が英数字、`_`、`-` なら一致しない（`foo/TASK-58`、`TASK-58-foo` は対象外）。全角括弧と句読点は隣にあってよい。

`text` ノードだけを分割する。`code` / `inlineCode` / `link` / `linkReference` / `definition` / `html` の内側は走査しない。autolink 済みの `link` を触らないよう GFM の後ろに置く。alerts の原文照合（先頭 text の position）を壊さないよう alerts の後ろに置く。

出力は `<a class="kfm-task-ref" href="/{tenant}/projects/{KEY}/tasks/{KEY-N}">` である。存在は確認しない（描画時に API を呼ばない）。

テナントは `renderDescription(text, { taskRef: { tenant } })` で渡し、コアが vfile の `data.kfm.taskRef` へ載せる。tenant が無いか、`^[a-z0-9][a-z0-9-]{0,62}$`（大文字小文字を区別しない）に合わなければ何もしない。同じ本文でもテナントで href が変わるので、テナントはキャッシュキーに含まれる。

### クリックの扱い

分割ビューとオーバーレイ（`TaskDetailPane`）の中では、修飾キーなしの左クリックを横取りし、ページ遷移せず同じ領域の中で開く（`open-task-ref`）。規則は次のとおりである。

- オーバーレイは子タスクと同じく戻り先を積む
- タスク一覧は 1 プロジェクト分なので、別プロジェクトの KEY-N は詳細ページへ移動する
- カレンダーはテナント横断なのでオーバーレイで開く
- Cmd / Ctrl / Shift / 中クリックはブラウザの既定のままである
- vike は document の click でリンクを横取りし `defaultPrevented` を見ないので、伝播も止める

## メンションのチップ（remark-kfm-mention）

本文中のメンションのマーカー `<@利用者ID>`（ハイフン付き UUID）をチップ `@今の名` にする。形式は入力欄と backend の `payload::mentions` と同じで、壊れたマーカーは素のテキストのまま残す。

- 表（id から状態、名前、アバターを引く）は `renderDescription` の `mentions` で渡し、vfile の `data.kfm.mentions` から読む。`mentions` を渡さない描画では何もせず、マーカーは素の文字のまま残る。空の表を渡すと、マーカーは「不明な利用者」のチップになる
- text ノードはチップへ分割する。code、inlineCode、link、linkReference の内側ではチップにせず `@今の名` の文字へ戻す。画像の alt、リンクと画像と定義の title、code の meta も同じである。URL と生の HTML は意味を変えない
- 状態は koyori の固定 class（`koyori-mention-member` / `-outside` / `-gone` / `-unknown`。`mentionClassNames` が決める）で表す。`currentUserId` と同じ member には `koyori-mention-self` も付ける
- `mentionTenant` を渡すと、今も参加しており名前の引けるチップを `/{tenant}/users/{利用者ID}` への link にする。`taskRef` とは別の引数なのは、コメントでは KEY-N をリンクにせずチップだけを link にするためである。`kfm-mention-tenant-routes.test.ts` が、表を渡す描画は必ずテナントも渡すことを固定している
- 表の内容で出力が変わるので、表、`currentUserId`、`mentionTenant` はキャッシュキーに含まれる

## コードブロックの枠（rehype-kfm-code）

着色済みのコードブロックを `<kfm-code>` で包み、帯（言語名とファイル名）の `data-lang` / `data-title` と行番号用の行分割（`pl-line`）を SSR の HTML へ出力する。

- 行分割は着色済みの span を行ごとに切る処理なので、rehype 層の starry-night の後ろに置く。mermaid フェンスは remark 層が `pre > code` を出さないので、このプラグインの対象にならない
- ファイル名は ` ```ts title="src/foo.ts" ` と ` ```ts:src/foo.ts ` のどちらでも書け、両方あれば `title="…"` を採る。`:` 形式は sanitize の `language-*` の字種に合わず、着色もされない
- コピーボタンは client の custom element（`element.ts`）が足し、SSR の HTML には入れない。sanitize の許可を `button` へ広げないためである。JS が無効ならボタンだけが無い
- コピーは code の `textContent` を書き込む。行番号は CSS の `::before` で描くのでコピーに混ざらない。VRT と E2E は `data-kfm-code-copy`（`copied` / `failed`）を待つ

## Mermaid の図（remark-kfm-mermaid）

### client 専用の描画

SSR は不活性な `<kfm-mermaid>` とエスケープ済みのソーステキストだけを出す。mermaid 本体は custom element の connectedCallback 内の dynamic import だけが参照し、図のあるページでだけ読み込まれる。

### securityLevel

securityLevel は strict に固定し、緩める変更はしない。`suppressErrorRendering` で構文エラー時のエラー図の DOM 注入も止め、失敗は状態値で表す。strict でも `click A "https://…"` の URL リンクは `<a>` として出力される。strict が止めるのは callback の実行で、リンクではない。

### shadow DOM

shadow DOM は描画の成功時にだけ張る。描画に成功した SVG は shadow に入れ、ページ側の CSS から隔離する。

### 完了シグナル

完了シグナルは `data-kfm-mermaid` の二値（`rendered` / `error`）で、属性なしは未処理である。VRT と E2E はこの属性の出現を待ち、時間待ちはしない。状態を増やすときは、待つ側（story play、VRT）を全部洗い出してから変更する。

### 挿入前の検査

挿入前の最後の検査は、sink と同じ HTML パースで行う（`element.ts` の `parseSafeSvg`）。検査を通った同一ノードをそのまま挿入する。

href のスキームの拒否は要素の種類で分ける。`<a>`（遷移の sink）と画像以外の href は実行可能スキーム（`javascript:` / `vbscript:` / `data:` 全体）を拒否する。`<image>` / `<img>` は画像として解釈される sink なので `data:image/` だけを許す。

### 一時描画コンテナ

一時描画コンテナは自要素内の不可視ノードである。接続中の自要素内へ置いて、成否にかかわらず撤去する。この方式の副作用を二つ手当てしている。

- 描画ソースは textContent でなく直下の text node だけから組む。描画中の再接続でコンテナの中身がソースへ混入しないためである
- テキスト計測は祖先の CSS を継承するので、非 rendered 状態専用のサイドカー CSS の `white-space: pre` だけはコンテナ側で打ち消す

### テーマ

テーマは描画時のスナップショットである。`.dark` の祖先を描画時に一度だけ見る。

## サニタイズ（_sanitize.ts）

DOMPurify は HTML 構造の allowlist に専念する。

- `USE_PROFILES: { html: true }` で SVG と MathML を落とす
- `FORBID_ATTR: ['style']` で inline style を経路を問わず落とす
- class は既知のトークンとの完全一致で許可する（`afterSanitizeAttributes` フック）。許可集合の定義元は、各プラグインが export する `SanitizeSchema` を `createRenderer({ sanitizeSchemas })` で合成した registry である
- 動的な class は `SanitizeSchema.classPatterns` で明示した固定形だけを許可する。現在はコードフェンスの `language-*` と starry-night の `pl-*` に限り、任意のアプリの class を通す汎用パターンにはしない
- `CUSTOM_ELEMENT_HANDLING` は registry への登録制である。現在は `kfm-code`（属性は `data-lang` と `data-title`）と `kfm-mermaid`（属性なし）を許可する。`allowCustomizedBuiltInElements: false` で `is=""` の経路を塞ぐ
- `classPatterns` の正規表現に `g` / `y` フラグは使えない。`lastIndex` の状態で `.test()` の判定が呼び出し履歴により反転するので、registry の組み立て時に throw する

DOMPurify を最終段に置くのは、remark プラグインの出力を含む最終 HTML 文字列を、`v-html` へ渡す直前の一か所で検査するためである。class allowlist のフックは DOMPurify のモジュール singleton を汚さないよう sanitize の呼び出し中だけ登録し、`finally` で撤去する。

### URI とリンク属性

URI とリンク属性の扱いは、DOMPurify の既定に依拠する部分も含めて次のとおりである。

- `data:` URI は画像系タグ（`img` など）の `src` に限り通る（DOMPurify の既定）。`a href` の `data:` は通らない。GitHub と同様に埋め込み画像の表示を許容する。サイズ上限などの資源制約は KFM 層では課さない
- 本文のリンクに `rel`（`nofollow noopener noreferrer` など）を付けず、`target` も出力しない。リンクが `target` / `rel` を持たないことはテストで固定している

## キャッシュ（_cache.ts）

`renderDescription` はモジュール singleton であり、サーバーでは全リクエストと全 tenant で共有される。この前提で、L1 のキーは入力本文そのもの（full-text）とし、ハッシュ化しない。

キーの前置部は pipeline fingerprint、profile、scope、taskRef のテナント、解決済みの content-scope config、メンションの表である。scope をキーに含めるのは安全条件の一部で、落とすと `clobberPrefix` の違う HTML を取り違え、別の断片の脚注 id が付いた HTML を返す。テナントも同様で、落とすと別テナントの href が付いた HTML を返す。

fingerprint は plugin 列と sanitize スキーマから導出するので、構成を変えると古い HTML は自動で失効する。fingerprint から見える非配列の plugin の情報は `plugin.name` だけで、closure に閉じた設定値は見えない。factory に options を追加するときは、設定の直列化とキャッシュキーの分離を同じ変更で実装する。

`lru-cache` は `max`（500 件）、`maxSize`（4 MiB）、`sizeCalculation`（キーと値の UTF-8 バイト長の合計）で上限を持つ。

## SSR と Hydration

### 描画元テキストとの照合

`+data.ts` を持つ描画箇所（タスク詳細、ドキュメント詳細）は、サーバー生成の HTML だけを受ける。`descriptionHtml: await renderDescription(text)` を実行し、描画元テキスト `descriptionSource`（renderDescription へ渡した入力そのもの）と対で `pageContext.data` に載せる。

呼び出し側は descriptionSource がクライアントの持つ最新の生テキストと厳密に一致するときだけ descriptionHtml を `v-html` へ流す。一致しないとき（保存直後、reload の失敗、他者の更新で HTML が古いとき）はプレーンテキストで表示する。descriptionHtml を単独で受けて無条件に `v-html` する呼び出し側は作らない。

### 保存後の再描画

タスクの編集を保存した後に KFM 表示へ戻すには、同じ URL へ `navigate(href, { keepScrollPosition: true, overwriteLastHistoryEntry: true })` して `+data.ts` を再実行する。`reload()` はスクロール位置を保てないので使わない。`overwriteLastHistoryEntry: true` で履歴エントリを置き換えるので、保存のたびにブラウザの戻るが 1 段増えることは無い。`keepScrollPosition` だけでは履歴が積み上がる。

実装の定義元は `task-description-navigation.ts` の `TASK_DESCRIPTION_REFRESH_NAVIGATE_OPTIONS` である。失敗すると descriptionSource の照合が一致せず、プレーンテキストのまま残る。

### 入力長の上限

入力長の上限は、呼び出し側が `renderDescription` を呼ぶ前に敷く。renderDescription の CPU も L1 キャッシュのメモリも本文長に比例するので、サーバーに上限の無い入力を入れない。超過すると `descriptionHtml: null` などでプレーンテキスト表示になり、エラーは出さない。値は[描画する箇所](#描画する箇所)の表のとおりである。

タスク詳細は backend の保存上限 `payload::tasks::MAX_DESCRIPTION_LEN`（100,000）に手で合わせており、backend は code point、フロントエンドは UTF-16 単位で数える。内部エンドポイント、ドキュメント詳細、コメントは 65,536（GitHub issue 本文の上限と同値）のままである。コメントの長さは backend が制限しないので、上限を超えるコメントも保存はでき、表示だけがプレーンになる。

### backend からの取得の時間上限

サーバーの描画箇所で KFM 描画の前に backend からの取得が要るなら、直列のリクエストごとの timeout ではなく、取得全体で共有する上限を設ける。現在は 3 秒の単一の `AbortSignal` である。超過時は `descriptionHtml: null` としてプレーンテキスト表示にし、ページ本体の表示と編集には影響させない。これは backend の取得待ちだけの上限であり、SSR 全体の完了時間を保証するものではない。

### scope

同じページに複数の KFM 断片を並べるときは、断片ごとに決定的な scope を渡す（`renderDescription(text, { scope: 'comment-42' })`）。

scope は `[A-Za-z0-9_-]+` だけを許可し、それ以外は throw する。加えて、`-` 区切りのセグメントとして `fn` / `fnref` を含む scope（`fn-1`、`a-fn-b` など）も throw する。脚注 id は `user-content-<scope>-fn-<label>` で label は利用者の入力なので、細工した label と別の scope の id が一致し得るためである。scope の側からセグメントを禁止すれば衝突しない。

scope 付き描画では脚注 id（`fn-*` / `fnref-*`）に加え、`footnote-label` とそれを指す `aria-describedby` にも scope が付く。scope なしの既定の描画は GitHub 互換の固定の `footnote-label` を保つ。

### 改行の正規化

入口で `\r\n` と単独の `\r` を `\n` へ正規化する（キーの構築より前）。正規化しないと alert のマーカー照合が CRLF の本文で成立せず、LF 版と CRLF 版が別の HTML、別のキャッシュエントリになる。

### サーバーとブラウザの分担

サーバーで描画した HTML を、クライアントが再パースや再サニタイズすることは無い。DOMPurify はサーバーで一度だけ走る。

コメントとレビュー指摘は取得がクライアント操作で `+data.ts` が使えないので、ブラウザで `import('@/lib/markup-renderer')` して描画する。描画中と失敗時はプレーンテキストを表示する。この経路ではパイプライン一式がブラウザで動き、starry-night はブラウザ向けの実装（`https://esm.sh/vscode-oniguruma@2/release/onig.wasm` を取得する）を読み込む。

カスタム要素の登録は `src/pages/+client.ts`（client 専用の entry）から行い、関数側にも `customElements` 不在のガードを持たせる。main.ts は存在せず、`+onCreateApp.ts` は SSR でも走るので使わない。

## クライアントからの描画（POST /internal/render-description）

選択がクライアント操作の画面（タスク一覧の分割ビュー）や入力中のプレビュー（ドキュメントの編集）は、`+data.ts` が追従できない。これらは Elysia の内部エンドポイント（`apps/frontend/server/middlewares/render-description.ts`）へ本文を送り、HTML だけを受け取る。KFM 一式を client bundle へ載せない（+417.5 KB）ためのエンドポイントである。

入口では描画の前に二つを確認する。CPU を使う処理を誰でも実行できる状態にしないためである。

- `content-length` が無ければ `411`、上限（本文上限の 8 倍）を超えれば本文を読まず `413` を返す
- backend の `/v1/auth/me` でセッションを確かめ（10 秒キャッシュ）、ログインしていなければ `401` を返す。未ログインの訪問者にも `session` Cookie が発行されるので、Cookie の有無では判定しない

本文の扱いは次のとおりである。

- 受け取るのは `{ description, scope, tenant?, mentions? }` である
- `scope` は断片ごとの決定的な識別子で、`+data.ts` と同じ組み立てにする（タスク説明は `task-<uuid>`、ドキュメントは `doc-<file_id>`）。`[A-Za-z0-9_-]{1,64}` に合わなければ `400` を返し、スキーマ違反の `422` とは分ける。`fn` / `fnref` セグメントを含む scope は renderDescription が throw するので、`html: null` を返す
- `tenant` は任意で、`renderDescription` の `taskRef` と `mentionTenant` に渡す。字種は `^[A-Za-z0-9][A-Za-z0-9-]{0,62}$` で検査し、外れれば `422` を返す
- `mentions` は任意で、チップを描くための表である（200 件まで）
- 旧形式の `{ description, taskId }`（`scope = task-<taskId>`）も受ける
- 呼び出し側（`composables/useRenderedDescription.ts` の `renderOnServer`）は `400` / `411` / `413` / `422` を「その入力では通らない」として `html: null` で記憶し、`401` と `5xx` は再試行する

## 利用方法

```ts
// +data.ts（サーバー側）: 実体は tasks/@taskId/+data.ts。scope はタスク UUID で決定的にする。
// taskRef と mentionTenant は URL の tenant、mentions は API の応答の表である。
// HTML は描画元テキストと必ず対で返す（呼び出し側の照合キー）
import { renderDescription } from '@/lib/markup-renderer';
const descriptionHtml = await renderDescription(task.description, {
  scope: `task-${task.id}`,
  taskRef: { tenant: tenantDisplayId },
  mentions: task.mentions ?? {},
  mentionTenant: tenantDisplayId,
});
return { descriptionHtml, descriptionSource: task.description };

// 呼び出し側のコンポーネントで CSS を明示的に import する
import '@/lib/remark-gfm/style.css';
import '@/lib/remark-koyori-alerts/style.css';
import '@/lib/rehype-starry-night/style.css';
import '@/lib/rehype-kfm-code/style.css';
import '@/lib/remark-kfm-mermaid/style.css';
```

```html
<!-- 呼び出し側のコンポーネント: 描画元が最新の生テキストと厳密に一致するときだけ v-html する。
     ラッパーに kfm-content を付ける（GFM CSS の scope） -->
<div
  v-if="descriptionHtml && descriptionSource === task.description"
  class="kfm-content"
  v-html="descriptionHtml"
/>
<!-- 不一致と HTML なしはプレーンテキストで表示する -->
<p v-else>{{ task.description }}</p>
```

### サイドカー CSS

サイドカー CSS は、プラグインごとに置いて呼び出し側が明示的に import する CSS である。GFM CSS だけがラッパーのクラスを要し、他は import だけで当たる。

- GFM CSS は import とラッパーのクラスの二点が要る。GFM の出力は素の ul/ol/blockquote/a/del で掴む class が無く、bare 要素へ当てるとアプリ全体へ漏れるので、全ルールが `.kfm-content` の子孫に限定されている。クラスを付け忘れると一行も当たらない
- alerts CSS は、レンダラ自身が出力する名前空間クラス（`.kfm-alert` など）を直接指す
- メンションのチップは koyori の固定 class で描き、GFM CSS の `a` の規則は `:not(.koyori-mention)` でチップを外す
- 着色 CSS は starry-night が出力する `pl-*` の名前空間クラスを直接指す。実体は upstream の light シート固定と `.dark` ブリッジで、アプリの class 戦略のダークに追従する
- kfm-code CSS は `kfm-code` 要素と `.pl-line` を直接指し、着色 CSS と同じ場所で import する
- mermaid CSS は JS 無効時または描画失敗時の light DOM のソースを、`white-space: pre` と横スクロールで読める状態に保つ

ラッパーのクラスの定義元は `remark-gfm/content-class.ts`（`KFM_CONTENT_CLASS`）である。CSS との scope の一致は `kfm-gfm-css-contract.test.ts` が強制し、story のラッパーも同じ定数を使う（VRT baseline のラッパーは本番と同じになる）。

### 着色の失敗

着色の transformer の初期化や変換が reject すると、`renderDescription` も reject する。未着色コードへの部分的なフォールバックはしない。ただし `+data.ts` と内部エンドポイントはこれを捕まえて null を返すので、プレーンテキスト表示になる。呼び出し側が独自に処理するときも、失敗を握り潰さない。

## 拡張の方法

新しいフレーバーの追加は次の 3 点に閉じる。コア、sanitize、cache、SSR の決まりは共有のままである。

1. `_renderer.ts` の `KfmProfile` union にプロファイル名を足す
2. composition root（`index.ts`）の `profiles` にそのプロファイルの remark 層と rehype 層を注入する
3. 追加プラグインの `SanitizeSchema`（class、タグ、属性）を `_schemas.ts` の一覧に合流させる

`defaultProfile` に選べる値は `_config.ts` の `KNOWN_PROFILES`（現在は `github` のみ）に限られる。`comment` は呼び出し側が `profile` で明示して使う。

カスタム要素は `_client-registry.ts` の定義配列に追加し、対応するプラグインの出力、`SanitizeSchema.tags` / `attrs`、登録の三点を揃える。多層 config（`_config.ts`）は解決層を重ねる形で拡張する。

追加プラグインは生の HTML や inline style を出さず、`data.hName` / `hProperties` の型付きの出力と対応する `SanitizeSchema` を同じ変更で提供する。新しい class は完全一致のトークンを原則とする。動的な値が必要な場合だけ、許容する字種を狭く固定した `classPatterns` と陽性、陰性のテストを加える。

## テスト

テストは `src/lib/__tests__/kfm-*.test.ts` にあり、各ファイルの冒頭コメントに対象を書いている。ファイル数とテスト数は増え続けるので、現在値は `pnpm test:unit` の出力を定義元とする。

セキュリティ上の要点（inline style の禁止、full-text キー、client ガード）は、いずれも「その規約を破る変更を入れるとテストが落ちる」形で書かれている。

## story fixture

KFM の Storybook story (`stories/kfm/*`) は、本番と同じ「サーバー生成 HTML を v-html するだけ」の同期描画で VRT baseline を決定的にする。ラッパーも本番と同じ `.kfm-content`（`remark-gfm/content-class.ts` の定数を import）で、GFM のサイドカー CSS が本番と同じ条件で当たる。fixture の運用は次の四点である。

- 入力の定義元は `src/lib/kfm-story-fixtures/inputs.ts` と `inputs-mermaid.ts` である。キー 1 つが fixture 1 枚、story 1 つに対応するのが基本である。mermaid 分は `mermaid-` 接頭辞の別ファイルで、drift 検査は `kfm-mermaid-fixtures.test.ts` が行う
- `rendered/*.html` は `renderDescription` の事前生成物で、手で書き換えない
- 再生成は `pnpm test:unit --update` で行う（`kfm-story-fixtures.test.ts` の `toMatchFileSnapshot` が drift を CI で検出する）
- `vite.config.ts` の `fmt.ignorePatterns` から `rendered/**` を外さない。formatter が生成 HTML を整形すると、drift 検査が偽陽性で落ちる

## 未実装と既知の制限

- `kfm` profile（MFM や Koyori 拡張を含む）と `gitlab` profile。`KfmProfile` は `github` と `comment` だけである
- config の tenant / project / user 層と lock。`resolveContentConfig` はコード既定と system 層だけを解決する
- L2 キャッシュ（ブラウザ永続）
- mermaid の図の、描画後のテーマ切り替えへの追従
- 本文のリンクの `rel` の付与と `target` の出力。`target` を導入するときは `rel` の付与と同時に行う。`nofollow` は SEO の方針の問題であり、必要になった時点で rehype 層として追加する

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-08-18 | L1 キャッシュのキーをハッシュ化しない | 32-bit ハッシュは誕生日境界の約 77,000 件で衝突し、別 tenant の本文の HTML を返す漏えいになる |
| 2026-08-18 | L2 キャッシュ（ブラウザ永続）を採用しない | 必要性を計測してから設計する |
| 2026-08-20 | SVG をサーバーで生成する方式を採らず、Mermaid を client で描画する | mermaid はレイアウト計測に DOM を要するブラウザ描画である |
| 2026-08-20 | shadow DOM を失敗時に張らない | 張ると light DOM のソーステキスト（JS 無効時と失敗時のフォールバック表示）まで隠れる |
| 2026-08-20 | SVG の挿入前の検査に XML パーサを使わない | mermaid の正常出力（click 付き flowchart の `<a xlink:href>` は `xmlns:xlink` 宣言なし）を偽陽性で落とし、逆に HTML 再パースで構造が変わる mXSS を素通しする |
| 2026-08-20 | 着色 CSS に OS 設定に連動する both.css を使わない | 発火条件を `.dark` の一系統にまとめ、OS がダークでアプリがライトのときにコードだけ暗転する状態を防ぐ |
| 2026-08-20 | scope をランダムにしない | 同じ入力から同じ HTML を得る性質（L1 キャッシュ、SSR と CSR の一致）を保つ |
| 2026-08-21 | 本文のリンクに `rel` を付けない | `target="_blank"` を出力しないので reverse tabnabbing の経路が無く、同じタブの遷移では `noopener` は不要である |
| 2026-08-22 | `data:` を一律に拒否せず、`<image>` / `<img>` の `data:image/` を許す | C4 図が Person アイコンを `<image xlink:href="data:image/png;base64,…">` で埋める（mermaid@11.16.1 で実測）ので、一律に拒否すると C4 図全体が error になる |
| 2026-08-22 | 一時描画コンテナを自要素内に置く | render の第三引数を省くと mermaid が一時ノードを document.body の末尾へ置き、図が一瞬見える |
| 2026-10-16 | テナントをプラグインの options にせず vfile で渡す | processor は profile ごとに memoize されており、テナントごとに作るとメモリが漏れる |

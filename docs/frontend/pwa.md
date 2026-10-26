# PWA（アプリとしての install）仕様書

> ステータス: **Draft**
> 作成日: 2026-10-18
> 関連: TASK-297（この文書の範囲）、TASK-299（アイコン）、TASK-300（offline）、TASK-301（通知）

---

## 1. ねらいと範囲

別のアプリを入れずに、web の task をブラウザの枠の外の窓（standalone）で開けるようにする。

| 入れた物 | 入れておらぬ物 |
|---|---|
| Web App Manifest、icon と favicon（TASK-299、仮の絵）、`/sw.js`（何も cache しない）、install の案内のボタン | offline（TASK-300）、通知（TASK-301）、icon の清書、asset の先取り（precache） |

道具（vite-plugin-pwa や Serwist）は足していない。依存を増やさず、manifest と最小の service worker を手で置いた。

## 2. 置き場

| 物 | 場所 | 中身 |
|---|---|---|
| manifest の値・色・icon の道 | `apps/frontend/src/lib/pwa/app-manifest.ts` | 一か所に集めてある。`+Head.vue` と server がここから引く |
| manifest の配り | `apps/frontend/server/pwa.ts`（`pwaManifestPlugin`）、`server/elysia.ts` | `/manifest.webmanifest` を Elysia から返す |
| icon | `apps/frontend/public/pwa/` | `icon-192.png`、`icon-512.png`、`icon-maskable-512.png`、`apple-touch-icon.png`。favicon は `public/favicon-16.png` と `public/favicon-32.png` |
| icon の元の絵と焼き方 | `apps/frontend/design/app-icon/` | `source.png` と `build.py`。焼く時の決めは同じ dir の `README.md` |
| service worker | `apps/frontend/public/sw.js` | 後述 |
| `/sw.js` の header | `apps/frontend/server/pwa.ts`（`pwaStaticHeaders`）、`+server.ts` の `prod.middleware` | 後述 |
| 登録 | `apps/frontend/src/lib/pwa/register-service-worker.ts`、`src/pages/+client.ts` | client の entry から一度だけ |
| install の案内 | `apps/frontend/src/lib/pwa/install-prompt.ts`、`src/components/header/NavUser.vue` | 利用者の menu の「Install app」 |
| install の条件の確かめ | `apps/frontend/scripts/pwa-installability.mjs` | 7 章 |

## 3. manifest

| 欄 | 値 | 訳 |
|---|---|---|
| `id` | `/` | ブラウザが install 済みのアプリを見分ける鍵。変えると、install 済みの者には別のアプリになる。origin の根に置き、URL の構成が変わっても揺らがぬようにした |
| `start_url` | `/` | テナントへの振り分けは `/` の頁が行う。テナントや利用者を埋めると、別の者や別のテナントで開いた時に食い違う |
| `scope` | `/` | アプリの全ての頁を standalone の窓に収める。service worker の scope と揃える |
| `name` と `short_name` | `task` | |
| `display` | `standalone` | |
| `theme_color` | `#fafafa`（仮） | header の `--sidebar`（oklch(0.985 0 0)） |
| `background_color` | `#ffffff`（仮） | `--background`（oklch(1 0 0)） |
| `icons` | 192、512、maskable 512 | 元の絵（`design/app-icon/source.png`）を焼いた物（TASK-299、仮の絵）。不透明で、透けは白で埋める |

**`id`・`start_url`・`scope` は後で変えない。**

色と icon の差し替え（TASK-299）は、`app-manifest.ts` の二つの色と、`public/pwa/` の画像を同じ名で置き換えるだけで済む。

`+Head.vue` は SSR の HTML に次の四つを載せる。

- `<link rel="manifest">`
- `<meta name="theme-color">`
- `<link rel="apple-touch-icon">`
- `<meta name="apple-mobile-web-app-title">`

## 4. service worker

`/sw.js` を scope `/` で置く。

- **何も cache しない。** fetch の受け手を置かないので、どの request も service worker が無い時と同じく network へ行く。API の応答・認証の絡む頁・静的な物のいずれも、別の利用者や別のテナントの中身、古い権限の画面が残る道は無い。
- **更新**: 新しい版は `install` で `skipWaiting()`、`activate` で `clients.claim()` し、待たずに入れ替わる。cache を持たぬので古い画面が残る道は元から無い。それでも古い service worker が待ちの列に留まらぬようにしてある。
- **`/sw.js` は消さない。** この server は無い path に SSR の HTML を返す。ゆえに `/sw.js` を消すと更新の確かめが失敗し、ブラウザは今の service worker を持ち続ける。やめる時は、自らを unregister する版を `/sw.js` に置く。
- **登録**は `+client.ts`（client 専用の entry）から一度だけ。SSR では走らない。`vike dev` では登録しない。何も cache しないので害は無いが、同じ port の localhost に登録が残るのを避けるため。

## 5. 配信の header

| path | Content-Type | Cache-Control | 誰が付けるか |
|---|---|---|---|
| `/manifest.webmanifest` | `application/manifest+json; charset=utf-8` | `no-cache` | Elysia（`pwaManifestPlugin`） |
| `/sw.js` | `text/javascript; charset=utf-8` | `no-cache`（304 の時も） | 本番の server の middleware（`pwaStaticHeaders`） |

本番の server（`bun ./dist/server/index.mjs`、Dockerfile の CMD）では、`dist/client` の静的な物は Elysia より前に `@universal-deploy/node`（srvx の staticMiddleware）で配られる。

- srvx の mime の表は `.webmanifest` を知らず、`application/octet-stream` で返す。ゆえに manifest は public/ に置かず、Elysia から返す。
- srvx は既定で `Cache-Control` を付けない。ゆえに `/sw.js` の header は `+server.ts` の `prod.middleware` で揃える。
- 応答が HTML の時（`/sw.js` が無い時）は触らない。HTML を JavaScript と偽らぬため。

本番の前段（Dokploy の Traefik）が header を書き換えぬかは、repo の外の設定で確かめておらぬ。

## 6. install の案内

- **ブラウザの既定の案内を残す。** `beforeinstallprompt` で `preventDefault()` を呼ばない。呼ぶと Chrome の mobile の既定の案内（mini-infobar）が出なくなる。desktop のアドレスバーの install の印は、どちらでも出る。
- **自前のボタン**: 利用者の menu（`NavUser.vue`）の「Install app」。`beforeinstallprompt` を受け取った時だけ出し、押せば取っておいた event の `prompt()` を呼ぶ。event は一度しか使えないので、押した後は消える。
- install 済みでは出さない。standalone の窓で開いておる時（`display-mode: standalone`、iOS は `navigator.standalone`）と、`appinstalled` の後である。
- **iOS と iPadOS の Safari**: `beforeinstallprompt` が無いので、ボタンは出さない。「ホーム画面に追加」は Safari の共有の menu から利用者が行う。案内の文を出すかは、後の決めとする。

## 7. 確かめ方

- **unit 試験**:
  - `server/__tests__/pwa.test.ts`、`pwa-app.test.ts`: header と manifest
  - `src/lib/pwa/__tests__/`: 登録が一度だけで SSR と dev で走らぬこと、ボタンの出る条件
  - `src/components/header/__tests__/NavUser.test.ts`
- **build した物での install の条件**:

  ```sh
  cd apps/frontend
  NODE_ENV=production pnpm run build
  NODE_ENV=production PORT=4173 APP_URL=http://127.0.0.1:4173 bun ./dist/server/index.mjs &
  node scripts/pwa-installability.mjs http://127.0.0.1:4173/
  ```

  - 見る物は、Chromium の `Page.getInstallabilityErrors` が空、`beforeinstallprompt` の発火、`/sw.js` が scope `/` で activated、Cache Storage が空、の四つ。外れれば exit 1。
  - 陽性対照: `+Head.vue` から manifest の link を外して build すると、`no-manifest` が出て exit 1 になる。
  - 既定の headless shell と incognito の文脈では install の判定が働かないので、script は完全な Chromium を永続の profile で起こす。

## 8. 約束と未定の物

- **通知の許可は、利用者の明示の操作（設定の画面のボタンなど）の中でだけ求める。** 頁を開いただけで求めない。iOS と iPadOS では、ホーム画面に足した web app でも、利用者の操作への応答でなければ許可を求められない。
- 未定:
  - TASK-299: 正式な icon（maskable、apple-touch-icon を含む）と色
  - TASK-300: offline（やるなら precache と、API・認証の頁を掴ませぬ取り決めから）
  - TASK-301: 通知
- 通知（TASK-301）で足す物:
  - `/sw.js` の `push` と `notificationclick` の受け手
  - VAPID の鍵（秘密は backend、公開は frontend）
  - backend の購読の置き場と API
  - 許可を求める UI
  - logout とテナントの切り替えでの購読の解除

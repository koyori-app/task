---
title: PWA
description: Web の task を standalone のウィンドウで開くための manifest、service worker、install の案内
updated: 2026-11-04
sidebar:
  order: 5
---

別のアプリを入れずに、Web の task をブラウザの枠の外のウィンドウ（standalone）で開けるようにする。Web App Manifest、アイコンと favicon（TASK-299、仮の絵）、`/sw.js`、install を案内するボタンを手で置いている。この文書の範囲は TASK-297 である。offline（TASK-300）と通知（TASK-301）は範囲外で、[未実装と既知の制限](#未実装と既知の制限)の節に書く。

## 置き場

| 対象 | 場所 | 内容 |
|---|---|---|
| manifest の値、色、アイコンのパス | `apps/frontend/src/lib/pwa/app-manifest.ts` | 一か所に集める。`+Head.vue` とサーバーがここから読む |
| manifest の配信 | `apps/frontend/server/pwa.ts`（`pwaManifestPlugin`）、`server/elysia.ts` | `/manifest.webmanifest` を Elysia から返す |
| アイコン | `apps/frontend/public/pwa/` | `icon-192.png`、`icon-512.png`、`icon-maskable-512.png`、`apple-touch-icon.png`。favicon は `public/favicon-16.png` と `public/favicon-32.png` |
| アイコンの元絵と生成スクリプト | `apps/frontend/design/app-icon/` | `source.png` と `build.py`。生成時の決め事は同じディレクトリの `README.md` にある |
| service worker | `apps/frontend/public/sw.js` | [service worker](#service-worker)の節を参照 |
| `/sw.js` のヘッダー | `apps/frontend/server/pwa.ts`（`pwaStaticHeaders`）、`+server.ts` の `prod.middleware` | [配信のヘッダー](#配信のヘッダー)の節を参照 |
| 登録 | `apps/frontend/src/lib/pwa/register-service-worker.ts`、`src/pages/+client.ts` | client の entry から一度だけ行う |
| install の案内 | `apps/frontend/src/lib/pwa/install-prompt.ts`、`src/components/header/NavUser.vue` | 利用者メニューの「Install app」 |
| install 条件の確認 | `apps/frontend/scripts/pwa-installability.mjs` | [確かめ方](#確かめ方)の節を参照 |

## manifest

各欄の値は次のとおりである。

| 欄 | 値 | 説明 |
|---|---|---|
| `id` | `/` | ブラウザが install 済みのアプリを識別するキーである |
| `start_url` | `/` | テナントへの振り分けは `/` のページが行う |
| `scope` | `/` | アプリの全ページを standalone のウィンドウに収める。service worker の scope と揃える |
| `name` と `short_name` | `task` | |
| `display` | `standalone` | |
| `lang` | `ja` | |
| `theme_color` | `#fafafa`（仮） | ヘッダーの `--sidebar`（oklch(0.985 0 0)）に合わせる |
| `background_color` | `#ffffff`（仮） | `--background`（oklch(1 0 0)）に合わせる |
| `icons` | 192、512、maskable 512 | 元絵（`design/app-icon/source.png`）から生成する（TASK-299、仮の絵）。不透明で、透明部分は白で埋める |

`id`、`start_url`、`scope` は後で変えない。

色とアイコンの差し替えは、`app-manifest.ts` の二つの色の変更と、`public/pwa/` と `public/` の画像を同じ名前で置き換えることで済む。

`+Head.vue` は favicon に加えて、SSR の HTML に次の四つを出力する。

- `<link rel="manifest">`
- `<meta name="theme-color">`
- `<link rel="apple-touch-icon">`
- `<meta name="apple-mobile-web-app-title">`

## service worker

`/sw.js` を scope `/` で登録する。

### キャッシュ

`/sw.js` は何もキャッシュしない。fetch のリスナーを置かないので、どのリクエストも service worker が無いときと同じくネットワークへ行く。

### 更新

新しい版は `install` で `skipWaiting()`、`activate` で `clients.claim()` を呼び、待たずに入れ替わる。古い service worker が待機状態に留まることも避けるためである。

`/sw.js` は消さない。やめるときは、自身を unregister する版を `/sw.js` に置く。

### 登録

登録は `+client.ts`（client 専用の entry）から一度だけ行い、SSR では走らない。`vike dev` では登録しない。登録に失敗しても reject せず、次の呼び出しで再度試みる。

## 配信のヘッダー

パスごとのヘッダーは次のとおりである。

| path | Content-Type | Cache-Control | 付与する場所 |
|---|---|---|---|
| `/manifest.webmanifest` | `application/manifest+json; charset=utf-8` | `no-cache` | Elysia（`pwaManifestPlugin`） |
| `/sw.js` | `text/javascript; charset=utf-8` | `no-cache`（304 のときも） | 本番サーバーの middleware（`pwaStaticHeaders`） |

本番サーバー（`bun ./dist/server/index.mjs`、Dockerfile の CMD）では、`dist/client` の静的ファイルは Elysia より前に `@universal-deploy/node`（srvx の staticMiddleware）が配信する。この構成から次の三つの扱いが決まる。

- srvx の MIME の表は `.webmanifest` を持たず、`application/octet-stream` で返す。そのため manifest は public/ に置かず、Elysia から返す。
- srvx は既定で `Cache-Control` を付けない。そのため `/sw.js` のヘッダーは `+server.ts` の `prod.middleware` で揃える。
- `/sw.js` が無く応答が HTML になったときは、ヘッダーを変更しない。

## install の案内

ブラウザの既定の案内を残すため、`beforeinstallprompt` で `preventDefault()` を呼ばない。デスクトップのアドレスバーの install のアイコンは、どちらでも出る。

独自のボタンは利用者メニュー（`NavUser.vue`）の「Install app」である。`beforeinstallprompt` を受け取ったときだけ出し、押すと保持していたイベントの `prompt()` を呼ぶ。イベントは一度しか使えないので、押した後は消える。

install 済みではボタンを出さない。standalone のウィンドウで開いているとき（`display-mode: standalone`、iOS は `navigator.standalone`）と、`appinstalled` の後がこれに当たる。

iOS と iPadOS の Safari には `beforeinstallprompt` が無いので、ボタンは出さない。「ホーム画面に追加」は利用者が Safari の共有メニューから行う。

## 確かめ方

ユニットテストは `server/__tests__/pwa.test.ts`、`pwa-app.test.ts`、`src/lib/pwa/__tests__/`、`src/components/header/__tests__/NavUser.test.ts` にある。

ビルド成果物での install 条件は次の手順で確かめる。

```sh
cd apps/frontend
NODE_ENV=production pnpm run build
NODE_ENV=production PORT=4173 APP_URL=http://127.0.0.1:4173 bun ./dist/server/index.mjs &
node scripts/pwa-installability.mjs http://127.0.0.1:4173/
```

確認するのは次の四つである。いずれかを満たさなければ exit 1 で終わる。

- Chromium の `Page.getInstallabilityErrors` が空である
- `beforeinstallprompt` が発火する
- `/sw.js` が scope `/` で activated である
- Cache Storage が空である

陽性対照として、`+Head.vue` から manifest の link を外してビルドすると、`no-manifest` が出て exit 1 になる。

既定の headless shell と incognito のコンテキストでは install の判定が働かないので、スクリプトは完全な Chromium を永続プロファイルで起動する。

## 未実装と既知の制限

- TASK-299: 正式なアイコン（maskable、apple-touch-icon を含む）と色。今のアイコンと色は仮である
- TASK-300: offline とアセットの事前キャッシュ（precache）。実装するなら precache と、API や認証の絡むページをキャッシュしない取り決めから決める
- TASK-301: 通知
- 本番の前段（Dokploy の Traefik）がヘッダーを書き換えないかは、リポジトリの外の設定であり確認していない
- iOS と iPadOS の Safari で「ホーム画面に追加」の案内文を出すかは未定である

通知の許可は、利用者の明示的な操作（設定画面のボタンなど）の中でだけ求める。ページを開いただけでは求めない。iOS と iPadOS では、ホーム画面に追加した Web アプリでも、利用者の操作への応答でなければ許可を求められないためである。

通知（TASK-301）で追加するものは次のとおりである。

- `/sw.js` の `push` と `notificationclick` のリスナー
- VAPID の鍵（秘密鍵は backend、公開鍵は frontend）
- backend の購読の保存先と API
- 許可を求める UI
- ログアウトとテナント切り替えでの購読の解除

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-10-17 | vite-plugin-pwa や Serwist を導入せず、manifest と最小の service worker を手で置く | 依存を増やさない |
| 2026-10-17 | `id` を `/` にする | 変えると、install 済みの利用者には別のアプリとして扱われる。origin のルートに置き、URL の構成が変わっても動かないようにする |
| 2026-10-17 | `start_url` を `/` にする | テナントや利用者を埋め込むと、別の利用者や別のテナントで開いたときに食い違う |
| 2026-10-17 | service worker は何もキャッシュしない | API の応答、認証の絡むページ、静的ファイルのいずれについても、別の利用者や別のテナントの内容、古い権限の画面を残さない |
| 2026-10-17 | `/sw.js` を消さない | このサーバーは存在しないパスに SSR の HTML を返す。消すと更新の確認が失敗し、ブラウザは今の service worker を持ち続ける |
| 2026-10-17 | `vike dev` では service worker を登録しない | キャッシュしないので害は無いが、同じポートの localhost に登録が残ると DevTools の表示が紛らわしくなる |
| 2026-10-17 | manifest を public/ に置かず Elysia から返す | srvx の MIME の表に `.webmanifest` が無く、`application/octet-stream` で返る |
| 2026-11-04 | `/sw.js` が HTML になったときはヘッダーを変更しない | HTML を JavaScript と偽ると、壊れた service worker を据えることになる（日付は不明） |
| 2026-10-17 | `beforeinstallprompt` で `preventDefault()` を呼ばない | 呼ぶと Chrome のモバイルの既定の案内（mini-infobar）が出なくなる |

# syntax=docker/dockerfile:1
# frontend イメージ（Vike + Elysia SSR）。
# pnpm monorepo（catalog はルート pnpm-workspace.yaml）なのでビルドコンテキストはリポジトリルート。
# build 時に openapi.json から API 型（src/generated/api.d.ts は gitignore）を生成する。
#
# vike build の成果物は開発でも配布でも同じなので、runtime と production は同一内容。
# backend 側の Dockerfile と target 名を揃えるためにステージ名だけ分けてある。
#
# Node のバージョンは .github/workflows/frontend-build.yml の node-version と合わせること。

########## builder ##########
FROM node:26-bookworm-slim AS builder
# pnpm の版はルート package.json の packageManager フィールドに従わせる（二重管理を避ける）
RUN corepack enable
WORKDIR /repo

# ワークスペース一式をコピー（node_modules 等は .dockerignore で除外済み）
COPY . .

# pnpm の store をキャッシュマウントに載せ、再ビルド時のパッケージ再取得を避ける
RUN --mount=type=cache,target=/pnpm-store \
    pnpm install --frozen-lockfile --store-dir /pnpm-store --filter frontend-vike...

WORKDIR /repo/apps/frontend
# openapi.json（コミット済み）→ 型生成 → ビルド
RUN pnpm run openapi:generate
# ブラウザ側 Sentry の DSN（sentry.browser.config.ts の import.meta.env.PUBLIC_ENV__SENTRY_DSN）。
# vike build がバンドルへ埋め込む値なので、実行時の環境変数では変えられない。
# ブラウザに配る前提の公開値なのでイメージに入ってよい。未指定なら空（送信しない）。
# install のレイヤーキャッシュを崩さないよう、build の直前で宣言する
ARG PUBLIC_ENV__SENTRY_DSN=
RUN pnpm run build

# 実行時に要る node_modules 。SSR の依存は vite.config.ts の ssr.noExternal で dist/server に
# 同梱してあり、外部に残しているのは ssr.external の 2 つだけ:
# - isomorphic-dompurify（jsdom が __dirname 相対でファイルを読むため同梱できない）
# - @wooorm/starry-night（実行時に vscode-oniguruma の onig.wasm を node_modules から読む）
# その閉包だけを、pnpm が lockfile どおりに入れた node_modules から写す（版を解き直さない）。
# ワークスペース全体の node_modules（devDependencies 込みで 900 MB 超）はイメージに入れない。
# 外部に残すパッケージを増やしたら、ここの引数と runtime の確かめにも足すこと。
RUN node scripts/collect-runtime-deps.mjs /runtime-deps/node_modules \
    isomorphic-dompurify @wooorm/starry-night

########## runtime ##########
# 実行は Bun。e2e（e2e/playwright.config.ts）も `bun ./dist/server/index.mjs` で
# 起動しており、SSR は Elysia なので実行環境をそちらに揃える。
# ビルド側は vike / pnpm の都合で Node のままにしている。
FROM oven/bun:1-slim AS runtime
WORKDIR /repo/apps/frontend

# dist/server に SSR の依存を同梱してあるので、node_modules は builder で作った外部分だけ
COPY --from=builder --chown=bun:bun /runtime-deps/node_modules ./node_modules
COPY --from=builder --chown=bun:bun /repo/apps/frontend/dist ./dist
COPY --from=builder --chown=bun:bun /repo/apps/frontend/server/public ./server/public
COPY --from=builder --chown=bun:bun /repo/apps/frontend/package.json ./package.json

# 外部に残した依存が、このイメージの node_modules だけで動くことをビルド時に確かめる
# （e2e は全部入りの node_modules で動くので、ここの欠落を拾えない）。
# starry-night の初期化は onig.wasm を読み、dompurify の初期化は jsdom を起こす。
RUN bun -e 'const { createStarryNight, common } = await import("@wooorm/starry-night"); \
    const sn = await createStarryNight(common); \
    if (!sn.flagToScope("ts")) throw new Error("starry-night: ts の文法が無い"); \
    const purify = (await import("isomorphic-dompurify")).default; \
    if (purify.sanitize("<b onclick=x>ok</b>") !== "<b>ok</b>") throw new Error("dompurify: 期待と違う出力");'

# bun イメージに同梱の非 root ユーザー
USER bun
ENV NODE_ENV=production
ENV PORT=3000
EXPOSE 3000
CMD ["bun", "./dist/server/index.mjs"]

########## production: 配布用 ##########
# runtime と同一内容。backend の Dockerfile と同じ target 名で参照できるようにするためのエイリアス。
FROM runtime AS production

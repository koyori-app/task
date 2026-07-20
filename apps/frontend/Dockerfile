# syntax=docker/dockerfile:1
# 開発・動作確認用の frontend イメージ（Vike + Elysia SSR）。
# pnpm monorepo（catalog はルート pnpm-workspace.yaml）なのでビルドコンテキストはリポジトリルート。
# build 時に openapi.json から API 型（src/generated/api.d.ts は gitignore）を生成する。
#
# Node のバージョンは .github/workflows/frontend-build.yml の node-version と合わせること。

########## builder ##########
FROM node:24-bookworm-slim AS builder
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
RUN pnpm run build

########## runtime ##########
# 実行は Bun。e2e（e2e/playwright.config.ts）も `bun ./dist/server/index.mjs` で
# 起動しており、SSR は Elysia なので実行環境をそちらに揃える。
# ビルド側は vike / pnpm の都合で Node のままにしている。
FROM oven/bun:1-slim AS runtime
WORKDIR /repo/apps/frontend

# pnpm の node_modules は /repo/node_modules（.pnpm ストア）への相対 symlink 構成なので両方コピーする
COPY --from=builder --chown=bun:bun /repo/node_modules /repo/node_modules
COPY --from=builder --chown=bun:bun /repo/package.json /repo/pnpm-workspace.yaml /repo/
COPY --from=builder --chown=bun:bun /repo/apps/frontend/node_modules ./node_modules
COPY --from=builder --chown=bun:bun /repo/apps/frontend/dist ./dist
COPY --from=builder --chown=bun:bun /repo/apps/frontend/server/public ./server/public
COPY --from=builder --chown=bun:bun /repo/apps/frontend/package.json ./package.json

# bun イメージに同梱の非 root ユーザー
USER bun
ENV NODE_ENV=production
ENV PORT=3000
EXPOSE 3000
CMD ["bun", "./dist/server/index.mjs"]

# syntax=docker/dockerfile:1
# 開発・動作確認用の frontend イメージ（Vike + Elysia SSR）。
# pnpm monorepo（catalog はルート pnpm-workspace.yaml）なのでビルドコンテキストはリポジトリルート。
# build 時に openapi.json から API 型（src/generated/api.d.ts は gitignore）を生成する。

########## builder ##########
FROM node:22-bookworm-slim AS builder
RUN corepack enable && corepack prepare pnpm@11.13.0 --activate
WORKDIR /repo

# ワークスペース一式をコピー（node_modules 等は .dockerignore で除外済み）
COPY . .

# frontend とその依存だけ入れる
RUN pnpm install --frozen-lockfile --filter frontend-vike...

WORKDIR /repo/apps/frontend
# openapi.json（コミット済み）→ 型生成 → ビルド
RUN pnpm run openapi:generate
RUN pnpm run build

########## runtime ##########
FROM node:22-bookworm-slim AS runtime
WORKDIR /repo/apps/frontend

# pnpm の node_modules は /repo/node_modules（.pnpm ストア）への相対 symlink 構成なので両方コピーする
COPY --from=builder /repo/node_modules /repo/node_modules
COPY --from=builder /repo/package.json /repo/pnpm-workspace.yaml /repo/
COPY --from=builder /repo/apps/frontend/node_modules ./node_modules
COPY --from=builder /repo/apps/frontend/dist ./dist
COPY --from=builder /repo/apps/frontend/server/public ./server/public
COPY --from=builder /repo/apps/frontend/package.json ./package.json

ENV PORT=3000
EXPOSE 3000
CMD ["node", "./dist/server/index.mjs"]

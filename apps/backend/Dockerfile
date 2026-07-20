# syntax=docker/dockerfile:1
# backend イメージ。用途ごとに最終ステージを使い分ける。
#   runtime    … 開発・動作確認用（debug ビルド + psql 同梱）。docker-compose.yml が使う
#   production … 配布用（release ビルド。psql と run-migration.sh は含まない）
# ステージを省略すると最後の production になるので、開発用は target を明示すること。
#
# Rust のバージョンはリポジトリルートの rust-toolchain.toml と合わせること。
# このイメージのビルドコンテキストは apps/backend なので rust-toolchain.toml はコンテキストに
# 含まれず、ここでの指定が唯一の版指定になる（rust-toolchain.toml を上げても自動追従しない）。

########## builder 共通 ##########
FROM rust:1.95-bookworm AS builder-base
# .cargo/config.toml が Linux ビルドで mold リンカ（-fuse-ld=mold）を要求する
RUN apt-get update \
    && apt-get install -y --no-install-recommends mold \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build

# ワークスペース一式（crates/ + migration/）をコピー
COPY . .

########## builder: debug ##########
# cargo のレジストリと target をキャッシュマウントに載せ、再ビルド時の依存クレート
# 再取得・再コンパイルを避ける。キャッシュマウントの中身は次のステージへ COPY できないため、
# 成果物のバイナリだけ /out に取り出す。
FROM builder-base AS builder-debug
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    mkdir -p /out \
    && cargo build --bin backend \
    && cp target/debug/backend /out/backend

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/migration/target \
    cargo build --manifest-path migration/Cargo.toml \
    && cp migration/target/debug/migration /out/migration

########## builder: release ##########
FROM builder-base AS builder-release
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    mkdir -p /out \
    && cargo build --release --bin backend \
    && cp target/release/backend /out/backend

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/migration/target \
    cargo build --release --manifest-path migration/Cargo.toml \
    && cp migration/target/release/migration /out/migration

########## runtime 共通 ##########
FROM debian:bookworm-slim AS runtime-base
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# root では動かさない
RUN useradd --system --create-home --uid 10001 app
WORKDIR /app

########## runtime: 開発・動作確認用 ##########
FROM runtime-base AS runtime
# psql は run-migration.sh の DROP INDEX で使う
RUN apt-get update \
    && apt-get install -y --no-install-recommends postgresql-client \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder-debug --chown=app:app /out/backend /app/backend
COPY --from=builder-debug --chown=app:app /out/migration /app/migration
COPY --chown=app:app --chmod=755 docker/run-migration.sh /app/run-migration.sh

USER app
EXPOSE 3400
CMD ["/app/backend"]

########## production: 配布用 ##########
# run-migration.sh の DROP INDEX は開発時の SeaORM schema sync 向けの処理なので含めない。
# マイグレーションは同梱の /app/migration を明示的に実行して適用する。
FROM runtime-base AS production
COPY --from=builder-release --chown=app:app /out/backend /app/backend
COPY --from=builder-release --chown=app:app /out/migration /app/migration

USER app
EXPOSE 3400
CMD ["/app/backend"]

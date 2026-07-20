# syntax=docker/dockerfile:1
# 開発・動作確認用の backend イメージ（debug ビルド）。
# backend 本体とマイグレーション CLI（ワークスペースから exclude された別クレート）を両方ビルドする。
#
# Rust のバージョンはリポジトリルートの rust-toolchain.toml と合わせること。
# このイメージのビルドコンテキストは apps/backend なので rust-toolchain.toml はコンテキストに
# 含まれず、ここでの指定が唯一の版指定になる（rust-toolchain.toml を上げても自動追従しない）。

########## builder ##########
FROM rust:1.95-bookworm AS builder
# .cargo/config.toml が Linux ビルドで mold リンカ（-fuse-ld=mold）を要求する
RUN apt-get update \
    && apt-get install -y --no-install-recommends mold \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build

# ワークスペース一式（crates/ + migration/）をコピー
COPY . .

# cargo のレジストリと target をキャッシュマウントに載せ、再ビルド時の依存クレート
# 再取得・再コンパイルを避ける。キャッシュマウントの中身は次のステージへ COPY できないため、
# 成果物のバイナリだけ /out に取り出す。
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    mkdir -p /out \
    && cargo build --bin backend \
    && cp target/debug/backend /out/backend

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/migration/target \
    cargo build --manifest-path migration/Cargo.toml \
    && cp migration/target/debug/migration /out/migration

########## runtime ##########
FROM debian:bookworm-slim AS runtime
# psql は run-migration.sh の DROP INDEX で使う
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# root では動かさない
RUN useradd --system --create-home --uid 10001 app
WORKDIR /app

COPY --from=builder --chown=app:app /out/backend /app/backend
COPY --from=builder --chown=app:app /out/migration /app/migration
COPY --chown=app:app --chmod=755 docker/run-migration.sh /app/run-migration.sh

USER app
EXPOSE 3400
CMD ["/app/backend"]

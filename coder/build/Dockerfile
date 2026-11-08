# task の開発用 Coder ワークスペースのイメージ。
#
# /home/coder はワークスペースごとのボリュームで覆われるので、ここで入れるものは
# /home/coder の外に置く。自己更新するツール（Claude Code / Codex / Cursor CLI / T3 Code / Paradev）は
# main.tf の startup_script がホームへ入れる。
FROM ubuntu:24.04

# rust-toolchain.toml と合わせる。ずれても rustup がリポジトリの指定版を自動で取得する
ARG RUST_VERSION=1.95.0
# .node-version と合わせる
ARG NODE_VERSION=24.22.0

# mold は apps/backend/.cargo/config.toml が要求する。iproute2 / procps は scripts/restart_*_dev.sh、
# docker は統合テストのハーネス（testcontainers）と docker compose が使う
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev mold \
        ca-certificates curl git gh gnupg openssh-client sudo unzip xz-utils \
        ripgrep jq less vim iproute2 procps postgresql-client \
        docker.io docker-compose-v2 docker-buildx \
    && rm -rf /var/lib/apt/lists/*

# Tailscale（公式の apt リポジトリ）。状態は起動スクリプトがホームに置く
RUN curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/noble.noarmor.gpg -o /usr/share/keyrings/tailscale-archive-keyring.gpg \
    && curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/noble.tailscale-keyring.list -o /etc/apt/sources.list.d/tailscale.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends tailscale \
    && rm -rf /var/lib/apt/lists/*

# 起動スクリプトが非対話で clone できるよう、GitHub のホスト鍵（公式 API が公開しているもの）を登録する
RUN curl -fsSL https://api.github.com/meta | jq -r '.ssh_keys[] | "github.com \(.)"' > /etc/ssh/ssh_known_hosts

# pnpm は corepack が package.json の packageManager の版を使う
RUN arch="$(uname -m | sed 's/x86_64/x64/;s/aarch64/arm64/')" \
    && curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${arch}.tar.xz" \
        | tar -xJ -C /usr/local --strip-components=1 \
    && corepack enable

# ubuntu:24.04 は UID 1000 を ubuntu ユーザーが使っている
RUN userdel -r ubuntu \
    && useradd --create-home --shell /bin/bash --uid 1000 --groups docker coder \
    && echo 'coder ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/coder \
    && install -d -o coder -g coder /opt/rustup /opt/cargo

# CARGO_HOME は実行時に既定の ~/.cargo へ戻し、レジストリのキャッシュをホームのボリュームに残す。
# npm -g の行き先もホームにして、Codex を sudo なしで更新できるようにする
ENV RUSTUP_HOME=/opt/rustup \
    PATH=/home/coder/.local/bin:/home/coder/superset/bin:/home/coder/.cargo/bin:/opt/cargo/bin:$PATH \
    NPM_CONFIG_PREFIX=/home/coder/.local \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0 \
    LANG=C.UTF-8

# Python と uv（uv は必要な版の Python も自分で入れられる）
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-venv python3-pip \
    && rm -rf /var/lib/apt/lists/* \
    && curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR=/usr/local/bin INSTALLER_NO_MODIFY_PATH=1 sh

USER coder
WORKDIR /home/coder

RUN export CARGO_HOME=/opt/cargo \
    && curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" -c rustfmt,clippy \
    && curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
        | bash \
    && cargo binstall -y cargo-nextest cargo-hakari

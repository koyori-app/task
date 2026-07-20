#!/usr/bin/env bash
# check-docker-versions.sh -- Dockerfile の版指定が参照元とずれていないか検査する
# 使用法: bash scripts/check-docker-versions.sh
#
# Docker のビルドコンテキストには rust-toolchain.toml が含まれず、node のバージョンも
# ワークフロー側とは独立に書かれている。そのため参照元だけを上げても Docker のビルドは
# そのまま成功してしまい、ずれたことに気付けない。ここで明示的に突き合わせる。
set -euo pipefail

fail=0

check() {
  local label="$1" expected="$2" actual="$3" expected_src="$4" actual_src="$5"
  if [ "$expected" = "$actual" ]; then
    printf 'OK   %-6s %s (%s) == %s (%s)\n' "$label" "$expected" "$expected_src" "$actual" "$actual_src"
  else
    printf 'NG   %-6s %s (%s) != %s (%s)\n' "$label" "$expected" "$expected_src" "$actual" "$actual_src"
    fail=1
  fi
}

# Rust: rust-toolchain.toml は 1.95.0 形式、Dockerfile は 1.95 形式なので
# メジャー.マイナーで比較する
toolchain_full=$(grep -oP '^\s*channel\s*=\s*"\K[^"]+' rust-toolchain.toml)
toolchain_mm=$(printf '%s' "$toolchain_full" | grep -oP '^\d+\.\d+')
dockerfile_rust=$(grep -oP '^FROM rust:\K\d+\.\d+' apps/backend/Dockerfile | head -1)
check rust "$toolchain_mm" "$dockerfile_rust" rust-toolchain.toml apps/backend/Dockerfile

# Node: frontend-build.yml の node-version と Dockerfile の FROM node: をメジャーで比較する
ci_node=$(grep -oP "node-version:\s*'\K\d+" .github/workflows/frontend-build.yml | head -1)
dockerfile_node=$(grep -oP '^FROM node:\K\d+' apps/frontend/Dockerfile | head -1)
check node "$ci_node" "$dockerfile_node" frontend-build.yml apps/frontend/Dockerfile

if [ "$fail" -ne 0 ]; then
  echo
  echo 'Dockerfile の版指定が参照元とずれている。Dockerfile 側を合わせること。' >&2
  exit 1
fi

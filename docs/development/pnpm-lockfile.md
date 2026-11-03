---
title: "pnpm の lockfile 構成"
---

## 現在の構成

lockfile はリポジトリルートの `pnpm-lock.yaml` 一つにまとめている。
workspace のメンバー（`apps/frontend`、`packages/*`）は個別の lockfile を持たない。

`pnpm-workspace.yaml` には `sharedWorkspaceLockfile: true` を明示している。
pnpm の既定値も true だが、次の三つの理由で既定に頼らない。

- 既定値は pnpm のバージョンによって変わる可能性がある。
- 行があれば、まとめていることが意図であると読み手に伝わる。
- 同じファイルの `allowBuilds` が依存を一つずつ書き出す流儀であり、それに揃う。

例外は `e2e/` と `docs/` であり、どちらも `pnpm-workspace.yaml` の `packages` に載せず、自前の `pnpm-lock.yaml` を持つ。

- `e2e/` は独立したプロジェクトで、CI も `pnpm install --ignore-workspace` でインストールする。
- `docs/` は自前の `pnpm-workspace.yaml` も持つ。ルートの `overrides` が `vite` を vite-plus に差し替えるので、それを Astro が必要とする Vite に適用しないためである。

## なぜ以前は分かれていたのか

`sharedWorkspaceLockfile: false` は `apps/frontend` を workspace へ追加したとき（2026-06-27、17bc0e6c）に設定された。
コミットメッセージは「各 workspace が自前の lockfile を持つ」と述べるだけで、理由を記録していない。
同じコミットでルートの lockfile の内容が `apps/cli/pnpm-lock.yaml` へ移されており、当時 Node 製だった `apps/cli` が自前の lockfile を必要としたことが分けた理由とみられる。
その `apps/cli` の lockfile は Rust 製への置き換え（2026-09-02、e7cd8e44）で消え、以後は分けておく理由が残っていない。

## なぜまとめたのか

lockfile が分かれていると、catalog や overrides を変える PR がすべての lockfile を揃えて更新しなければならない。
実際、vite-plus 0.2.5 → 0.3.1 の Renovate PR はルートと `apps/frontend` の lockfile だけを更新した。
更新されなかった `packages/oxlint-plugin-api-path-params/pnpm-lock.yaml` に古い overrides が残り、CI の `pnpm install --frozen-lockfile` が `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH` で止まった。
ルートの一つにまとめれば設定と解決結果が一か所に揃うので、この形の失敗は起きない。

## 影響範囲

- CI のキャッシュキー（`hashFiles`）と `cache-dependency-path`、docker-build のパスフィルターはルートの lockfile を指す。e2e のキャッシュキーだけは `e2e/pnpm-lock.yaml` も含める。
- `apps/frontend/Dockerfile` はリポジトリルートをコンテキストとして `--filter` でインストールするので、lockfile の配置に依存しない。
- `.github/renovate.json5` に lockfile の配置に関する設定は無く、変更していない。
- `pnpm-workspace.yaml` の `allowBuilds` と `minimumReleaseAgeExclude` はルートの lockfile 一つに適用される。挙動は分かれていた頃と同じである。

---
title: pnpm の lockfile
description: lockfile をルートの一つにまとめる構成と例外、CI や Docker との関係
updated: 2026-11-04
sidebar:
  order: 2
---

lockfile はリポジトリルートの `pnpm-lock.yaml` 一つにまとめている。workspace のメンバー（`apps/frontend`、`packages/*`）は個別の lockfile を持たない。`pnpm-workspace.yaml` には `sharedWorkspaceLockfile: true` を明示している。

## 例外

`e2e/` と `docs/` は `pnpm-workspace.yaml` の `packages` に載せず、自前の `pnpm-lock.yaml` を持つ。

- `e2e/` は独立したプロジェクトで、CI も `pnpm install --ignore-workspace` でインストールする。
- `docs/` は自前の `pnpm-workspace.yaml` も持つ。ルートの `overrides` が `vite` を vite-plus に差し替えるので、それを Astro が必要とする Vite に適用しないためである。

## lockfile の配置に関わる設定

- CI のキャッシュキー（`hashFiles`）と `cache-dependency-path`、docker-build のパスフィルターはルートの lockfile を指す。e2e のキャッシュキーだけは `e2e/pnpm-lock.yaml` も含める。
- `apps/frontend/Dockerfile` はリポジトリルートをコンテキストとして `--filter` でインストールするので、lockfile の配置に依存しない。
- `.github/renovate.json5` に lockfile の配置に関する設定は無い。
- `pnpm-workspace.yaml` の `allowBuilds` と `minimumReleaseAgeExclude` はルートの lockfile 一つに適用される。挙動は分かれていた頃と同じである。

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-06-27 | `apps/frontend` を workspace へ追加したときに `sharedWorkspaceLockfile: false` を設定した（17bc0e6c） | 記録なし。コミットメッセージは「各 workspace が自前の lockfile を持つ」と述べるだけである。同じコミットでルートの lockfile の内容が `apps/cli/pnpm-lock.yaml` へ移されており、当時 Node 製だった `apps/cli` が自前の lockfile を必要としたとみられる |
| 2026-09-02 | `apps/cli` を Rust 製へ置き換え、その lockfile が消えた（e7cd8e44） | 以後は lockfile を分けておく理由が残っていない |
| 2026-09-13 | lockfile をルートの一つにまとめた | 分かれていると、catalog や overrides を変える PR がすべての lockfile を揃えて更新しなければならない。vite-plus を 0.2.5 から 0.3.1 へ上げる Renovate PR はルートと `apps/frontend` の lockfile だけを更新した。更新されなかった `packages/oxlint-plugin-api-path-params/pnpm-lock.yaml` に古い overrides が残り、CI の `pnpm install --frozen-lockfile` が `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH` で止まった。ルートの一つなら設定と解決結果が一か所に揃い、この形の失敗は起きない |
| 2026-09-13 | pnpm の既定値も true だが、`sharedWorkspaceLockfile: true` を明示する | 既定値は pnpm のバージョンによって変わる可能性がある。行があれば、まとめていることが意図であると読み手に伝わる。同じファイルの `allowBuilds` が依存を一つずつ書き出す流儀であり、それに揃う |

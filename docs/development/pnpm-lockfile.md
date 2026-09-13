# pnpm の lockfile 構成

## 現在の構成

lockfile はリポジトリ根の `pnpm-lock.yaml` 一つに寄せている（`sharedWorkspaceLockfile` は pnpm の既定値 true）。
workspace の一員（`apps/frontend`・`packages/*`）は個別の lockfile を持たない。

例外は `e2e/` である。
e2e は `pnpm-workspace.yaml` の `packages` に載っていない独立したプロジェクトで、CI も `pnpm install --ignore-workspace` で入れている。
そのため `e2e/pnpm-lock.yaml` はそのまま残している。

## なぜ以前は割れていたのか

`sharedWorkspaceLockfile: false` は apps/frontend を workspace へ足した折（2026-06-27、17bc0e6c）に据えられた。
commit message は「各 workspace が自前の lockfile を持つ」と述べるのみで、理由は記録されていない。
同じ commit で root の lockfile の中身が `apps/cli/pnpm-lock.yaml` へ移されており、当時 Node 製だった apps/cli が自前の lockfile を要したことが割った理由とみられる。
その apps/cli は Rust 製への置き換え（2026-09-01、e7cd8e44）で lockfile ごと消えた。
以後、割っておく理由は残っていない。

## なぜ寄せたのか

lockfile が割れていると、catalog や overrides を変える PR が全ての lockfile を揃えて更新せねばならない。
実際、vite-plus 0.2.5 → 0.3.1 の Renovate PR は根と apps/frontend の lockfile だけを更新し、
取り残された `packages/oxlint-plugin-api-path-params/pnpm-lock.yaml` が古い overrides を抱えたまま、
CI の `pnpm install --frozen-lockfile` が `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH` で止まった。
根の一つに寄せれば、設定と解決が一箇所に揃い、この形の停止は起きない。

## 影響範囲

- CI の cache key（`hashFiles`）と `cache-dependency-path`、docker-build の path filter は根の lockfile を指す。
- `apps/frontend/Dockerfile` はリポジトリ根を context に `--filter` で入れており、lockfile の道に依らない。
- Renovate は pnpm workspace を自動で辿るため、設定の手直しは要らない。
- 供給網の検め（supply-chain policy・`allowBuilds`・`minimumReleaseAgeExclude`）は根の lockfile 一つに掛かる。
  挙動は割れていた頃と同じである。

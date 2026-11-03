# docs

仕様書サイト。[Nimbus](https://nimbus-docs.com/quickstart/)（Astro）で組む。

pnpm のワークスペースはリポジトリ直下と分かれているので、このディレクトリで実行する。

```bash
pnpm install
pnpm dev                  # http://localhost:4321/
pnpm build && pnpm preview # 検索は build の索引を読むので preview で確かめる
```

- ページは `features/` `frontend/` `development/` と `index.md`・`breadcrumbs.md`（`src/content.config.ts` の `pattern`）。階層がそのまま URL とサイドバーになる
- `title` は必須で、本文に `# 見出し` は書かない（タイトルが H1 として出る）
- `features/tasks/N.name.md` は番号を URL に出さないよう `slug` を、並びのために `sidebar.order` を書く
- サイト内リンクは `/features/drive` の形で書く（相対の `.md` は解決されない）。`apps/backend/docs/` はサイト外なので GitHub の URL にする

## 公開

Cloudflare Pages の `koyori` プロジェクト（https://koyori.pages.dev ）。ビルド設定はダッシュボードにある: ルート `docs`、コマンド `pnpm build`、出力 `dist`、`NODE_VERSION` は 22.12 以上。

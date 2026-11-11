# docs

仕様書サイト。[Nimbus](https://nimbus-docs.com/quickstart/)（Astro）で組む。

pnpm のワークスペースはリポジトリ直下と分かれているので、このディレクトリで実行する。

```bash
pnpm install
pnpm dev                  # http://localhost:4321/
pnpm build && pnpm preview # 検索は build の索引を読むので preview で確かめる
```

- ページは `dev/` の下と `index.md`（`src/content.config.ts` の `pattern`）。階層がそのまま URL とサイドバーになる
- `title` は必須で、本文に `# 見出し` は書かない（タイトルが H1 として出る）
- `dev/features/tasks/N.name.md` は番号を URL に出さないよう `slug` を、並びのために `sidebar.order` を書く
- 図は ` ```mermaid ` で書く（`src/lib/mermaid-fence.ts` が描画用に変換する）。Mermaid で表せない図は `public/diagrams/` に SVG を置く
- サイト内リンクは `/dev/features/drive` の形で書く（相対の `.md` は解決されない）
- 旧 URL からの転送は `public/_redirects` に書く
- 書き方の規則は `STYLE.md`、新しいページの雛形は `_template-spec.md`（仕様）と `_template-guide.md`（手順）。`pnpm lint:style` で機械的に見られる規則を検査する（CI の `docs-lint` と同じ）

## 公開

https://docs.koyori.app に出す。Cloudflare Pages の `koyori` プロジェクト（https://koyori.pages.dev ）でビルドする。ビルド設定はダッシュボードにある: ルート `docs`、コマンド `pnpm build`、出力 `dist`、`NODE_VERSION` は 22.12 以上。

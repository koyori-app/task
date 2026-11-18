/**
 * remark-kfm-permalink — KFM 拡張: GitHub のパーマリンクだけの段落を `<kfm-permalink>` で包む。
 *
 * 対象は「段落がその URL だけ」のとき (素の URL と `<URL>` の autolink)。`[文字](URL)` と
 * 文中の URL はリンクのまま残す (文の途中にコードのブロックを差し込まない)。
 *
 * サーバはコードを焼かない。出力は `<kfm-permalink><a href="URL">URL</a></kfm-permalink>` で、
 * 抜粋は client の custom element (element.ts) が backend から取って差し込む。
 * 描画結果はプロセス全体で共有する L1 キャッシュに本文をキーとして載る (_cache.ts) ので、
 * 見る人の権限で変わる中身 (非公開リポジトリのコード) を HTML に入れてはならない。
 * JS 無効・未連携・権限なしのときは、中のリンクがそのまま見える。
 *
 * options は受けない (rehype-kfm-link-favicon と同じ理由: 描画ごとの値を持たない)。
 * data 属性も出さない。client は中の `<a>` の href を同じ parseGithubPermalink で読む。
 */
import type { Properties } from 'hast';
import type { Root } from 'mdast';
import { visit } from 'unist-util-visit';
import type { SanitizeSchema } from '../markup-renderer/_sanitize';
import { parseGithubPermalink } from './_parse';
import { KFM_PERMALINK_TAG } from './_tag';

export { KFM_PERMALINK_TAG } from './_tag';
export { buildSnippetFence, languageFlagForPath, parseGithubPermalink } from './_parse';

// mdast-util-to-hast (remark-rehype 内部) と同一の Data 拡張 (remark-kfm-mermaid と同型)。
declare module 'mdast' {
  interface Data {
    hName?: string | undefined;
    hProperties?: Properties | undefined;
  }
}

export function remarkKfmPermalink() {
  return (tree: Root): void => {
    visit(tree, 'paragraph', (node, index, parent) => {
      // タスクリストの項目 (`- [ ] URL`) の先頭の段落は対象にしない。チェックボックスは先頭の <p> の
      // 中に入る約束で、<p> を替えるとチェックボックスだけの行とコードのブロックに分かれる
      if (parent?.type === 'listItem' && typeof parent.checked === 'boolean' && index === 0) return;
      const [link] = node.children;
      if (node.children.length !== 1 || link?.type !== 'link') return;
      const [text] = link.children;
      // `[文字](URL)` も子 1 個の link になる。文字が URL そのものである (autolink) ときだけ
      // 抜粋にする。書き手が付けた文字を消さない
      if (link.children.length !== 1 || text?.type !== 'text' || text.value !== link.url) return;
      if (parseGithubPermalink(link.url) === null) return;
      // 段落の <p> を <kfm-permalink> に替えるだけで、子 (リンク) は触らない
      node.data = { ...node.data, hName: KFM_PERMALINK_TAG };
    });
  };
}

/**
 * 本プラグインが emit するタグの許可宣言。属性は SSR 段では一切許可しない —
 * data-kfm-permalink は client が立てる状態であり、サーバ生成 HTML に現れてはならない。
 */
export const kfmPermalinkSanitizeSchema = {
  tags: [KFM_PERMALINK_TAG],
} as const satisfies SanitizeSchema;

/**
 * remark 側の半分: タスクリストの項目 (mdast の `listItem.checked` が真偽値のもの) について、
 * チェックボックスの `[` の位置を求め、`data.hProperties` に載せて remark-rehype へ渡す。
 * `index.ts` の rehype 側がそれを `input` へ移す。
 *
 * 位置は構文木から取る。`[` を原文から探す (項目の印の直後を見るなど) と、項目の先頭が
 * 参照定義 (`- [x]: http://e.com` の次の行に実際のチェックがある形) のとき別の `[x]` を拾う。
 * mdast-util-gfm-task-list-item は、チェックを取り除いた後の最初の段落の先頭の text を 1 字削って
 * `position.start` を進める。そのため次のどちらかになる:
 * - 先頭の子が text で、その開始が段落の開始と同じ (削った後の字が残った): 削られた 1 字
 *   (空白・タブ・改行) の手前から空白とタブを読み飛ばした先が `]` で、`[` はその 2 字前
 * - それ以外 (削った後に text が空になり取り除かれた): `[` は段落の開始そのもの
 * どちらも、位置の 3 字が `[ ]` / `[x]` / `[X]` で、checked と一致し、直後が空白か行末か本文の終わりであるときだけ採る。
 * 合わない項目には何も付けず、押せないまま残す。
 */
import type { Root } from 'mdast';
import { visit } from 'unist-util-visit';

const BOX = /^\[([ \txX])\]/;

export function remarkKfmTaskOffset() {
  return function remarkKfmTaskOffsetTransform(tree: Root, file: { value: unknown }): void {
    const source = String(file.value);
    visit(tree, 'listItem', (node) => {
      if (typeof node.checked !== 'boolean') return;
      const paragraph = node.children.find((child) => child.type === 'paragraph');
      const start = paragraph?.position?.start.offset;
      if (!paragraph || start === undefined) return;
      const head = paragraph.children[0];
      const shifted = head?.type === 'text' && head.position?.start.offset === start;
      // 削られた区切り (start - 1。空白・タブ・改行) の手前から、空白とタブを読み飛ばして `]` を探す。
      // `]` と先頭の text の間が 1 字とは限らない (`- [x] ` で行が終わり本文が次の行にあるとき、
      // 行末の空白は text に入らず、削られるのは改行で、`[` は 5 字以上前になる)
      let close = start - 2;
      while (close >= 0 && (source[close] === ' ' || source[close] === '\t')) close -= 1;
      const offset = shifted ? close - 2 : start;
      if (offset < 0) return;
      const box = BOX.exec(source.slice(offset, offset + 3));
      if (!box) return;
      const next = source[offset + 3];
      if (next !== undefined && next !== ' ' && next !== '\t' && next !== '\n') return;
      if ((box[1] === 'x' || box[1] === 'X') !== node.checked) return;
      const data = (node.data ??= {}) as { hProperties?: Record<string, unknown> };
      data.hProperties = { ...data.hProperties, dataKfmTaskOffset: String(offset) };
    });
  };
}

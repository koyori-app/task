/**
 * remark 側の半分: タスクリストの項目 (mdast の `listItem.checked` が真偽値のもの) について、
 * チェックボックスの `[` の位置を求め、`data.hProperties` に載せて remark-rehype へ渡す。
 * `index.ts` の rehype 側がそれを `input` へ移す。
 *
 * 位置は構文木から取る。`[` を原文から探す (項目の印の直後を見るなど) と、項目の先頭が
 * 参照定義 (`- [x]: http://e.com` の次の行に実際のチェックがある形) のとき別の `[x]` を拾う。
 * mdast-util-gfm-task-list-item は、チェックを取り除いた後の最初の段落の先頭の text を 1 字削って
 * `position.start` を進める。そのため次のどちらかになる:
 * - 先頭の子が text で、その開始が段落の開始と同じ (削った後の字が残った): `[` は開始の 4 字前
 *   (`[x]` の 3 字と、削った区切り 1 字)
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
      const offset = shifted ? start - 4 : start;
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

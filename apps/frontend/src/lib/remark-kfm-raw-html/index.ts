/**
 * remark-kfm-raw-html — markdown 中の生の HTML を、落とさず字として出す。
 *
 * markdown の構文として読まれた生 HTML（行中の tag・HTML の行で始まる段落・HTML のコメント）は
 * mdast の `html` ノードになる。remark-rehype は `allowDangerousHtml` を使わぬ限りこのノードを
 * 黙って捨てるので、`Vec<String>` の `<String>` や `<b>太字</b>`・`<!-- 注 -->` が画面から消えておった。
 * 本プラグインは `html` ノードを、同じ値の `text` ノードへ替える。字は出力で escape されるので、
 * 書いたとおりの字が画面に出る（`&lt;String&gt;`）。HTML として解される経路は増えぬ
 * （生 HTML を通す allowlist は無く、通さぬ方針のまま。許す tag の一覧は無い）。
 *
 * 規則（kfm-raw-html テストで固定）:
 * - 行中の `html`（phrasing の親: paragraph・heading・emphasis・strong・delete・link・tableCell 等）は
 *   `text` にする。隣の `text` と一つに繋ぐ（`a<b>c` が一つの text になる）
 * - 行で始まる `html`（block: root・blockquote・listItem 等の直下）は、`text` を一つ持つ
 *   `paragraph` にする。複数行の HTML の行は、一つの paragraph の中の改行つきの text になる
 * - `code` / `inlineCode` は `html` ノードではないので触れぬ（今どおり）
 *
 * 他の remark 層（alerts・mention・task-ref）より前に置く。後ろの層は替えた後の text を見る。
 * 出力は mdast ノードのみ（生 html・data.hName なし）。
 */
import type { Html, Parent, Root, RootContent, Text } from 'mdast';
import { SKIP, visit } from 'unist-util-visit';

/** 子に phrasing（行中の字）を取る mdast ノードの型。ここの直下の `html` は text にする */
const PHRASING_PARENTS = new Set([
  'paragraph',
  'heading',
  'emphasis',
  'strong',
  'delete',
  'link',
  'linkReference',
  'tableCell',
]);

function mergeAdjacentText(parent: Parent): void {
  const merged: RootContent[] = [];
  for (const child of parent.children as RootContent[]) {
    const last = merged[merged.length - 1];
    if (child.type === 'text' && last?.type === 'text') {
      last.value += child.value;
      if (last.position && child.position)
        last.position = { start: last.position.start, end: child.position.end };
      continue;
    }
    merged.push(child);
  }
  parent.children = merged as Parent['children'];
}

export function remarkKfmRawHtml() {
  return (tree: Root): void => {
    const touched = new Set<Parent>();
    visit(tree, 'html', (node: Html, index, parent) => {
      if (!parent || index === undefined) return;
      const text: Text = { type: 'text', value: node.value, position: node.position };
      const phrasing = PHRASING_PARENTS.has(parent.type);
      (parent.children as RootContent[])[index] = phrasing
        ? text
        : { type: 'paragraph', children: [text], position: node.position };
      if (phrasing) touched.add(parent);
      return SKIP;
    });
    for (const parent of touched) mergeAdjacentText(parent);
  };
}

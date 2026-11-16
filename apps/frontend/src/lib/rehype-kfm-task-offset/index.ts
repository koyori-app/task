/**
 * rehype-kfm-task-offset — KFM 拡張: タスクリストの `input[type=checkbox]` に、原文での
 * `[` の位置を `data-kfm-task-offset` として足す (TASK-539)。
 *
 * 目的: 閲覧表示のままチェックを切り替えるとき、原文のどの字を替えるかを構文木が持つ位置で
 * 決める。行を走査して数える近似 (task-checklist.ts) は切り替えに使わない。
 *
 * 規則:
 * - 対象は `task-list-item` の `li` のうち、`position.start.offset` を持つもの。
 *   `li` の位置から項目の印 (`-` `*` `+` `1.` `1)` と後続の空白) を読み飛ばした先の
 *   `[` を位置とする。印の読み飛ばしが合わない・`input` が見つからない・位置の字と checked が食い違う項目には
 *   何も付けず、
 *   押せないまま残す。
 * - `input` は詰まったリストでは `li` の直下、空行で離したリストでは `li > p` の先頭にある。
 * - 位置は、改行を LF に直した本文 (`_renderer.ts` の normalized) での JS の文字列添字。
 * - `disabled` は外さない。外すのは消費側 (TaskDetailHub) が hydrate 後に行う。
 * - 属性は `data-*` なので sanitize の宣言は要らない。生の HTML は字に escape されるため、
 *   本文からこの属性を書き込むことはできない。
 * - options は受け取らない。_renderer.ts の processor fingerprint はプラグインの関数名しか
 *   見ないため、closure に閉じた options はキャッシュキーに現れない (rehype-kfm-link-favicon と同じ)。
 */
import type { Element, ElementContent, Root } from 'hast';
import { visit } from 'unist-util-visit';

export { KFM_TASK_OFFSET_ATTR } from './_attr';

/** 項目の印と後続の空白。先読みで `[ ]` `[x]` `[X]` `[\t]` を確かめ、`[` の手前までの長さを得る */
const MARKER_BEFORE_BOX = /(?:[-*+]|\d{1,9}[.)])[ \t]+(?=\[[ \txX]\])/y;

function asCheckbox(node: ElementContent | undefined): Element | undefined {
  return node?.type === 'element' && node.tagName === 'input' && node.properties.type === 'checkbox'
    ? node
    : undefined;
}

function hasClass(node: Element, name: string): boolean {
  const className = node.properties.className;
  return Array.isArray(className) && className.includes(name);
}

function isBlankText(node: ElementContent): boolean {
  return node.type === 'text' && node.value.trim() === '';
}

/** 先頭の空白だけの text (ブロックの間の改行) を除いた最初の子 */
function firstContent(children: ElementContent[]): ElementContent | undefined {
  return children.find((child) => !isBlankText(child));
}

/** `li` の先頭にあるチェックボックス。`li` 直下か、先頭の `p` の先頭の子 */
function findCheckbox(li: Element): Element | undefined {
  const first = firstContent(li.children);
  if (first?.type !== 'element') return undefined;
  if (first.tagName === 'p') return asCheckbox(first.children[0]);
  return asCheckbox(first);
}

export function rehypeKfmTaskOffset() {
  return function rehypeKfmTaskOffsetTransform(tree: Root, file: { value: unknown }): void {
    const source = String(file.value);
    visit(tree, 'element', (node) => {
      if (node.tagName !== 'li' || !hasClass(node, 'task-list-item')) return;
      const start = node.position?.start.offset;
      if (start === undefined) return;
      // sticky で `start` 位置だけを照合する (項目ごとに本文を切り出すと長い本文で二乗になる)
      MARKER_BEFORE_BOX.lastIndex = start;
      const marker = MARKER_BEFORE_BOX.exec(source);
      if (!marker) return;
      const checkbox = findCheckbox(node);
      if (!checkbox) return;
      const offset = start + marker[0].length;
      // 位置の字が本当にそのチェックボックスのものか確かめる。項目の先頭の `[x]:` がリンク参照の
      // 定義として消費され、チェックボックスが次の行の `[ ]` になる形などは、字と checked が食い違う
      const checked = source[offset + 1] === 'x' || source[offset + 1] === 'X';
      if (checked !== (checkbox.properties.checked === true)) return;
      // hast の property 名 (`data-kfm-task-offset` 属性へ出る)
      checkbox.properties.dataKfmTaskOffset = String(offset);
    });
  };
}

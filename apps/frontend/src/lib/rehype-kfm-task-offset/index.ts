/**
 * rehype-kfm-task-offset — KFM 拡張: タスクリストの `input[type=checkbox]` に、原文での
 * `[` の位置を `data-kfm-task-offset` として足す (TASK-539)。
 *
 * 目的: 閲覧表示のままチェックを切り替えるとき、原文のどの字を替えるかを構文木が持つ位置で
 * 決める。行を走査して数える近似 (task-checklist.ts) は切り替えに使わない。
 *
 * 二段で行う:
 * - remark 側 (`_remark.ts`): mdast でチェックボックスになる項目 (`listItem.checked` が真偽値) だけから
 *   位置を求め、`data.hProperties.dataKfmTaskOffset` として li へ載せる。原文は探さない。
 * - rehype 側 (このファイル): li の `dataKfmTaskOffset` を、その項目の先頭の `input[type=checkbox]` へ移す。
 *   `input` は詰まったリストでは `li` の直下、空行で離したリストでは `li > p` の先頭にある。
 *   見つからなければ捨てる (属性は li に残さない)。
 * - 位置が決まらない項目には付けず、押せないまま残す。
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
export { remarkKfmTaskOffset } from './_remark';

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
  return function rehypeKfmTaskOffsetTransform(tree: Root): void {
    visit(tree, 'element', (node) => {
      if (node.tagName !== 'li') return;
      const offset = node.properties.dataKfmTaskOffset;
      if (offset === undefined) return;
      delete node.properties.dataKfmTaskOffset;
      const checkbox = hasClass(node, 'task-list-item') ? findCheckbox(node) : undefined;
      if (checkbox && typeof offset === 'string') checkbox.properties.dataKfmTaskOffset = offset;
    });
  };
}

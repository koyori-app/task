/**
 * `el` を縦に巻く最も近い祖先（`overflow-y` が `auto` か `scroll` の要素）。無ければ `window`。
 *
 * ガントは左の一覧と図を、頁の一つの縦のスクロールで一緒に動かす。arc-vue に外のスクロール域を
 * 渡すと、図は自分の域を持たずに行の高さのままになり、低性能の端末でもその域に見える行だけを描く。
 */
export function scrollParentOf(el: Element | null): HTMLElement | Window {
  for (let node = el?.parentElement ?? null; node; node = node.parentElement) {
    const overflowY = getComputedStyle(node).overflowY;
    if (overflowY === 'auto' || overflowY === 'scroll') return node;
  }
  return window;
}

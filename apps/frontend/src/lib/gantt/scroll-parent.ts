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

/**
 * 図の箱の中で、図を横に巻ける要素。arc の内側の域（`.koyori-gantt-scroll`）か箱そのもので、
 * どちらも図が収まって巻けなければ null。
 */
export function horizontalScrollerIn(box: HTMLElement | null): HTMLElement | null {
  if (!box) return null;
  const candidates = [box.querySelector<HTMLElement>('.koyori-gantt-scroll'), box];
  return candidates.find((el) => !!el && el.scrollWidth - el.clientWidth > 1) ?? null;
}

import { onScopeDispose, ref, type Ref } from 'vue';

import {
  CLICK_SUPPRESS_MS,
  PAN_SETTLE_MS,
  PAN_SLOP_PX,
  panDays,
  scrollFirst,
  wheelPanDelta,
} from '@/lib/gantt/pan';

/**
 * ガントの範囲を、横の wheel とタッチのスワイプで 1 日ずつ前後へ送る。
 * 図が箱より広く横に巻ける時は、先にその向きの端まで図を巻き、残りで日付を送る。
 * 送っている間は `panning` が立ち、操作が `PAN_SETTLE_MS` 止まると下りる（取得と URL の書き込みの合図）。
 * マウスのドラッグは扱わない（広い画面の図は字を選べるため）。
 */
export function useGanttPan(options: {
  /** 1 日の幅（px） */
  dayWidth: () => number;
  /** wheel の deltaMode が頁の時の 1 頁の px */
  pageWidth: () => number;
  /** 範囲を `days` 日送る。正は後の日付へ */
  shift: (days: number) => void;
  /** 図を横に巻ける要素。巻けなければ null */
  scroller: () => HTMLElement | null;
}): {
  panning: Ref<boolean>;
  onWheel: (event: WheelEvent) => void;
  onPointerDown: (event: PointerEvent) => void;
  onPointerMove: (event: PointerEvent) => void;
  onPointerEnd: (event: PointerEvent) => void;
  onClickCapture: (event: MouseEvent) => void;
} {
  const panning = ref(false);
  let restPx = 0;
  let drag: {
    id: number;
    lastX: number;
    startX: number;
    startY: number;
    moved: boolean;
  } | null = null;
  // この時刻（performance.now()）までの click を握り潰す。0 なら握り潰さない
  let suppressUntil = 0;

  // 副作用（取得・URL の書き込み）は `panning` の watch の側で行い、ここには足さない
  let settleTimer: ReturnType<typeof setTimeout> | undefined;
  function settle() {
    clearTimeout(settleTimer);
    settleTimer = setTimeout(() => {
      panning.value = false;
      // 端数は止まっても捨てない。1 刻みが 1 日の幅に届かない wheel を間を置いて回しても
      // 範囲が動くように、次の操作へ持ち越す（捨てるのは wheel の向きが変わった時と指の操作の時）
    }, PAN_SETTLE_MS);
  }
  onScopeDispose(() => {
    clearTimeout(settleTimer);
    drag = null;
  });

  function apply(deltaPx: number) {
    let rest = deltaPx;
    const box = options.scroller();
    if (box) {
      const scrolled = scrollFirst(box.scrollLeft, box.scrollWidth - box.clientWidth, rest);
      if (scrolled.scrollLeft !== box.scrollLeft) box.scrollLeft = scrolled.scrollLeft;
      rest = scrolled.restPx;
    }
    const next = panDays(restPx, rest, options.dayWidth());
    restPx = next.restPx;
    if (next.days !== 0) {
      panning.value = true;
      options.shift(next.days);
    }
    settle();
  }

  function onWheel(event: WheelEvent) {
    const delta = wheelPanDelta(event, options.pageWidth());
    // 縦のスクロールと拡大縮小は止めない
    if (delta === null) return;
    event.preventDefault();
    // 向きが変わったら、逆向きに貯めた端数を捨てる（戻した分で先に貯めた分を打ち消さない）
    if (delta !== 0 && restPx !== 0 && Math.sign(delta) !== Math.sign(restPx)) restPx = 0;
    apply(delta);
  }

  function onPointerDown(event: PointerEvent) {
    suppressUntil = 0;
    restPx = 0;
    // 2 本目の指（ピンチ）が来たら、進んでいるスワイプも止める
    if (!event.isPrimary) {
      drag = null;
      return;
    }
    if (event.pointerType === 'mouse') return;
    drag = {
      id: event.pointerId,
      lastX: event.clientX,
      startX: event.clientX,
      startY: event.clientY,
      moved: false,
    };
  }

  function onPointerMove(event: PointerEvent) {
    if (!drag || drag.id !== event.pointerId) return;
    if (!drag.moved) {
      const dx = Math.abs(event.clientX - drag.startX);
      const dy = Math.abs(event.clientY - drag.startY);
      if (dx <= PAN_SLOP_PX && dy <= PAN_SLOP_PX) return;
      // 縦が勝つ出だしは頁の縦スクロールに任せる
      if (dx <= dy) {
        drag = null;
        return;
      }
      drag.moved = true;
      suppressUntil = Number.POSITIVE_INFINITY;
      // 図の SVG は送るたびに差し替わるので、残る要素（受け口）に捕まえる
      (event.currentTarget as HTMLElement | null)?.setPointerCapture?.(event.pointerId);
    }
    apply(-(event.clientX - drag.lastX));
    drag.lastX = event.clientX;
  }

  function onPointerEnd(_event: PointerEvent) {
    const moved = drag?.moved ?? false;
    drag = null;
    // 指を離したら端数を捨てる（次の wheel に足さない）
    restPx = 0;
    // タッチの click は pointerup より遅れて届くので、時刻で抑える
    if (moved) suppressUntil = performance.now() + CLICK_SUPPRESS_MS;
  }

  // スワイプの後の click で、棒・群れの見出し・重ねた行の button が動かないようにする
  function onClickCapture(event: MouseEvent) {
    if (performance.now() >= suppressUntil) return;
    // キーボード（Enter / Space）で押した click は detail が 0。スワイプ由来ではないので通し、
    // 抑止も消さない
    if (event.detail === 0) return;
    event.preventDefault();
    event.stopPropagation();
    suppressUntil = 0;
  }

  return { panning, onWheel, onPointerDown, onPointerMove, onPointerEnd, onClickCapture };
}

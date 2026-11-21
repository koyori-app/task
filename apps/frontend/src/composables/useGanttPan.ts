import { ref, type Ref } from 'vue';
import { useDebounceFn } from '@vueuse/core';

import { PAN_SETTLE_MS, PAN_SLOP_PX, panDays, wheelPanDelta } from '@/lib/gantt/pan';

/**
 * ガントの範囲を、横の wheel とタッチのスワイプで 1 日ずつ前後へ送る。
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
  let drag: { id: number; lastX: number; startX: number; moved: boolean } | null = null;
  let suppressClick = false;

  const settle = useDebounceFn(() => {
    panning.value = false;
  }, PAN_SETTLE_MS);

  function apply(deltaPx: number) {
    const next = panDays(restPx, deltaPx, options.dayWidth());
    restPx = next.restPx;
    if (next.days !== 0) {
      panning.value = true;
      options.shift(next.days);
    }
    void settle();
  }

  function onWheel(event: WheelEvent) {
    const delta = wheelPanDelta(event, options.pageWidth());
    // 縦のスクロールと拡大縮小は止めない
    if (delta === null) return;
    event.preventDefault();
    apply(delta);
  }

  function onPointerDown(event: PointerEvent) {
    suppressClick = false;
    restPx = 0;
    if (event.pointerType === 'mouse' || !event.isPrimary) return;
    drag = { id: event.pointerId, lastX: event.clientX, startX: event.clientX, moved: false };
  }

  function onPointerMove(event: PointerEvent) {
    if (!drag || drag.id !== event.pointerId) return;
    if (!drag.moved) {
      if (Math.abs(event.clientX - drag.startX) <= PAN_SLOP_PX) return;
      drag.moved = true;
      suppressClick = true;
      // 図の SVG は送るたびに差し替わるので、残る要素（受け口）に捕まえる
      (event.currentTarget as HTMLElement | null)?.setPointerCapture?.(event.pointerId);
    }
    apply(-(event.clientX - drag.lastX));
    drag.lastX = event.clientX;
  }

  function onPointerEnd(_event: PointerEvent) {
    drag = null;
    // click は pointerup の直後に来るので抑え、来なかった時も次の click（キーボードの Enter）を握り潰さない
    if (suppressClick) {
      setTimeout(() => {
        suppressClick = false;
      }, 0);
    }
  }

  // スワイプの後の click で、棒・群れの見出し・重ねた行の button が動かないようにする
  function onClickCapture(event: MouseEvent) {
    if (!suppressClick) return;
    event.preventDefault();
    event.stopPropagation();
    suppressClick = false;
  }

  return { panning, onWheel, onPointerDown, onPointerMove, onPointerEnd, onClickCapture };
}

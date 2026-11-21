/** これを越えて横に動いたら、タッチをスワイプとみなす（px） */
export const PAN_SLOP_PX = 8;
/** 操作が止まってから、取得と URL の書き込みを行うまでの間（ms） */
export const PAN_SETTLE_MS = 200;
/** wheel の deltaMode が行の時の 1 行の px */
export const WHEEL_LINE_PX = 16;

/** 貯めた px に今回の移動を足し、送る日数と残りの px に分ける。正は後の日付へ */
export function panDays(
  restPx: number,
  deltaPx: number,
  dayWidth: number,
): { days: number; restPx: number } {
  if (!Number.isFinite(dayWidth) || dayWidth <= 0) return { days: 0, restPx: 0 };
  const total = restPx + deltaPx;
  // -0 を 0 に揃える
  const days = Math.trunc(total / dayWidth) || 0;
  return { days, restPx: total - days * dayWidth };
}

/** wheel を横の移動量（px）に読む。横の操作でなければ null */
export function wheelPanDelta(
  event: Pick<WheelEvent, 'deltaX' | 'deltaY' | 'deltaMode' | 'shiftKey' | 'ctrlKey'>,
  pageWidth: number,
): number | null {
  // ctrl + wheel は拡大縮小（トラックパッドのピンチもこれで届く）
  if (event.ctrlKey) return null;
  let delta: number;
  if (event.shiftKey && event.deltaX === 0) delta = event.deltaY;
  else if (Math.abs(event.deltaX) > Math.abs(event.deltaY)) delta = event.deltaX;
  else return null;
  if (event.deltaMode === 1) return delta * WHEEL_LINE_PX;
  if (event.deltaMode === 2) return delta * pageWidth;
  return delta;
}

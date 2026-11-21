import { describe, expect, it } from 'vitest';

import { panDays, wheelPanDelta, WHEEL_LINE_PX } from '@/lib/gantt/pan';

describe('panDays', () => {
  it('1 日の幅に届かぬ移動は送らず、px を貯める', () => {
    expect(panDays(0, 20, 30)).toEqual({ days: 0, restPx: 20 });
  });

  it('貯めた分と合わせて 1 日の幅を越えれば送る', () => {
    expect(panDays(20, 15, 30)).toEqual({ days: 1, restPx: 5 });
  });

  it('ちょうど 1 日の幅で 1 日送る', () => {
    expect(panDays(0, 30, 30)).toEqual({ days: 1, restPx: 0 });
  });

  it('負の向きは前の日付へ送る', () => {
    expect(panDays(0, -65, 30)).toEqual({ days: -2, restPx: -5 });
  });

  it('幅が 0 や有限でなければ送らず、貯めた分も捨てる', () => {
    expect(panDays(10, 50, 0)).toEqual({ days: 0, restPx: 0 });
    expect(panDays(10, 50, Number.NaN)).toEqual({ days: 0, restPx: 0 });
  });
});

describe('wheelPanDelta', () => {
  const wheel = (init: Partial<WheelEvent>) => ({
    deltaX: 0,
    deltaY: 0,
    deltaMode: 0,
    shiftKey: false,
    ctrlKey: false,
    ...init,
  });

  it('横が勝てば deltaX を返す', () => {
    expect(wheelPanDelta(wheel({ deltaX: 40, deltaY: 10 }), 800)).toBe(40);
  });

  it('縦が勝つか同じ大きさなら横の操作とみなさない', () => {
    expect(wheelPanDelta(wheel({ deltaX: 10, deltaY: 40 }), 800)).toBeNull();
    expect(wheelPanDelta(wheel({ deltaX: 20, deltaY: -20 }), 800)).toBeNull();
  });

  it('Shift + 縦のホイールは横に読む', () => {
    expect(wheelPanDelta(wheel({ deltaY: 100, shiftKey: true }), 800)).toBe(100);
  });

  it('ctrl 付きは拡大縮小なので読まない', () => {
    expect(wheelPanDelta(wheel({ deltaX: 40, ctrlKey: true }), 800)).toBeNull();
  });

  it('行と頁の単位を px に換える', () => {
    expect(wheelPanDelta(wheel({ deltaX: 3, deltaMode: 1 }), 800)).toBe(3 * WHEEL_LINE_PX);
    expect(wheelPanDelta(wheel({ deltaX: -1, deltaMode: 2 }), 800)).toBe(-800);
  });
});

import { describe, expect, it } from 'vitest';

import { diffDays, mondayOf } from '@/lib/calendar/timeline';

import {
  chartBackend,
  defaultFrom,
  fitDayWidth,
  GANTT_SCALES,
  SCHEDULE_MAX_DAYS,
  shiftView,
  viewDays,
  viewTo,
} from '../view';

describe('viewDays と shiftView', () => {
  it('どの目盛りも 7 の倍数で、送っても範囲の頭が月曜のまま', () => {
    for (const scale of GANTT_SCALES) {
      for (const compact of [false, true]) {
        expect(viewDays(scale, compact) % 7).toBe(0);
        const next = shiftView('2026-11-09', scale, compact, 1);
        expect(mondayOf(next)).toBe(next);
        expect(shiftView(next, scale, compact, -1)).toBe('2026-11-09');
      }
    }
  });

  it('日の目盛りは広い画面で 3 週、狭い画面で 1 週', () => {
    expect(viewTo('2026-11-02', 'day', false)).toBe('2026-11-22');
    expect(viewTo('2026-11-02', 'day', true)).toBe('2026-11-08');
    expect(diffDays('2026-11-02', viewTo('2026-11-02', 'month', false))).toBe(90);
  });

  it('どの範囲も予定の API が一度に引ける日数に収まる', () => {
    for (const scale of GANTT_SCALES) {
      for (const compact of [false, true]) {
        expect(viewDays(scale, compact)).toBeLessThanOrEqual(SCHEDULE_MAX_DAYS);
      }
    }
  });
});

describe('chartBackend', () => {
  const base = { compact: true, rowCount: 10, chartHeight: 600, screenHeight: 800 };

  it('広い画面は常に SVG', () => {
    expect(chartBackend({ ...base, compact: false, rowCount: 400, chartHeight: 20000 })).toBe(
      'svg',
    );
  });

  it('狭い画面は、行が 40 を越えるか、図が画面の 3 つ分より高い時だけ canvas', () => {
    expect(chartBackend(base)).toBe('svg');
    expect(chartBackend({ ...base, rowCount: 40 })).toBe('svg');
    expect(chartBackend({ ...base, rowCount: 41 })).toBe('canvas');
    expect(chartBackend({ ...base, chartHeight: 2400 })).toBe('svg');
    expect(chartBackend({ ...base, chartHeight: 2401 })).toBe('canvas');
  });
});

describe('fitDayWidth', () => {
  it('広い画面は箱の幅いっぱいに広げ、下限は割らない', () => {
    // 3 週で 692px の箱なら (692 - 20) / 21 = 32
    expect(fitDayWidth(692, 'day', false)).toBe(32);
    expect(fitDayWidth(1000, 'day', false)).toBe(46);
    expect(fitDayWidth(300, 'day', false)).toBe(30);
  });

  it('狭い画面は箱に収めるのを先にする', () => {
    // 1 週で 343px の箱なら (343 - 20) / 7 = 46
    expect(fitDayWidth(343, 'day', true)).toBe(46);
  });

  it('箱の幅が測れる前は下限の幅', () => {
    expect(fitDayWidth(0, 'week', false)).toBe(12);
  });
});

describe('defaultFrom（「今日」と既定の範囲）', () => {
  it('広い画面の日の目盛りは、今日の週の前に一週を残す（案 A の板は今日 11/8 を 11/2 からの範囲に置く）', () => {
    // 2026-11-11（水）の週の月曜は 11/9。その前の一週から
    expect(defaultFrom('2026-11-11', 'day', false)).toBe('2026-11-02');
    expect(defaultFrom('2026-11-08', 'day', false)).toBe('2026-10-26');
  });

  it('今日はどの目盛りでも範囲の内にあり、範囲の頭ではない（狭い画面の日の目盛りは今日の週）', () => {
    const today = '2026-11-11';
    for (const scale of GANTT_SCALES) {
      for (const compact of [false, true]) {
        const from = defaultFrom(today, scale, compact);
        expect(mondayOf(from)).toBe(from);
        expect(diffDays(from, today)).toBeGreaterThan(0);
        expect(diffDays(today, viewTo(from, scale, compact))).toBeGreaterThan(0);
      }
    }
    expect(defaultFrom(today, 'day', true)).toBe('2026-11-09');
  });
});

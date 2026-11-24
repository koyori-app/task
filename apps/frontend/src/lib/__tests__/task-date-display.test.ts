import { describe, expect, it } from 'vitest';
import {
  daysFromToday,
  formatDetailDate,
  formatMonthDay,
  remainingLabel,
} from '@/lib/task-date-display';

// 見る人の今日は 2026-11-16（月）
const today = new Date(2026, 10, 16, 15, 30);
const iso = (date: string) => `${date}T00:00:00.000Z`;

describe('formatDetailDate', () => {
  it('今年の日付は月日と曜日', () => {
    expect(formatDetailDate(iso('2026-11-16'), today)).toBe('11/16（月）');
    expect(formatDetailDate('2026-11-21', today)).toBe('11/21（土）');
  });

  it('今年でない日付だけ年を付ける', () => {
    expect(formatDetailDate(iso('2027-01-10'), today)).toBe('2027/1/10（日）');
    expect(formatDetailDate(iso('2025-12-31'), today)).toBe('2025/12/31（水）');
  });

  it('未設定は —', () => {
    expect(formatDetailDate(null, today)).toBe('—');
    expect(formatDetailDate(undefined, today)).toBe('—');
    expect(formatDetailDate('', today)).toBe('—');
  });

  it('暦の日は見る人の時間帯で動かさない（編集欄と同じ日）', () => {
    // UTC の 0 時は、西の時間帯では前の日になる。それでも 11/16 と出す
    expect(formatDetailDate('2026-11-16T00:00:00Z', today)).toBe('11/16（月）');
    expect(formatMonthDay('2026-11-16T00:00:00Z')).toBe('11/16');
  });
});

describe('remainingLabel', () => {
  it('30 日以内は「あと N 日」、31 日は出さない', () => {
    expect(remainingLabel(iso('2026-11-17'), today, false)).toEqual({
      text: 'あと 1 日',
      overdue: false,
    });
    expect(remainingLabel(iso('2026-12-16'), today, false)).toEqual({
      text: 'あと 30 日',
      overdue: false,
    });
    expect(daysFromToday(iso('2026-12-17'), today)).toBe(31);
    expect(remainingLabel(iso('2026-12-17'), today, false)).toBeNull();
  });

  it('当日は「今日」で、赤にしない', () => {
    expect(remainingLabel(iso('2026-11-16'), today, false)).toEqual({
      text: '今日',
      overdue: false,
    });
    // 日付の変わる直前でも当日
    expect(remainingLabel(iso('2026-11-16'), new Date(2026, 10, 16, 23, 59), false)?.text).toBe(
      '今日',
    );
  });

  it('過ぎたら「N 日過ぎ」を赤で出す', () => {
    expect(remainingLabel(iso('2026-11-15'), today, false)).toEqual({
      text: '1 日過ぎ',
      overdue: true,
    });
    expect(remainingLabel(iso('2026-10-01'), today, false)).toEqual({
      text: '46 日過ぎ',
      overdue: true,
    });
  });

  it('完了済みは、過ぎていても残っていても出さない', () => {
    expect(remainingLabel(iso('2026-11-15'), today, true)).toBeNull();
    expect(remainingLabel(iso('2026-11-20'), today, true)).toBeNull();
  });

  it('未設定は出さない', () => {
    expect(remainingLabel(null, today, false)).toBeNull();
  });
});

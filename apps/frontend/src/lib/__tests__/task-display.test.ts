import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import {
  clampProgressPct,
  formatDeadline,
  formatProgressPct,
  formatTaskDate,
  isoToLocalDateInput,
  localDateInputToIso,
  PRIORITY_CONFIG,
  taskDetailHref,
  taskListHref,
  taskSeqKey,
} from '../task-display';

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('taskSeqKey', () => {
  it('プロジェクトキーと連番から KEY-N を組み立てる', () => {
    expect(taskSeqKey('ENG', 42)).toBe('ENG-42');
  });
});

describe('taskDetailHref', () => {
  it('詳細ページ URL を生成する', () => {
    expect(taskDetailHref('acme', 'ENG', 42)).toBe('/acme/projects/ENG/tasks/ENG-42');
  });
});

describe('taskListHref', () => {
  it('一覧ページ URL を生成する', () => {
    expect(taskListHref('acme', 'ENG')).toBe('/acme/projects/ENG/tasks');
  });
});

describe('formatTaskDate', () => {
  it('ISO 日時を日本語表示に変換する', () => {
    const formatted = formatTaskDate('2026-07-01T12:00:00Z');
    expect(formatted).toContain('2026');
  });

  it('null/undefined は null を返す', () => {
    expect(formatTaskDate(null)).toBeNull();
    expect(formatTaskDate(undefined)).toBeNull();
  });
});

describe('formatDeadline', () => {
  it('1時間前の期限は超過と判定する（時刻比較）', () => {
    const now = new Date('2026-07-14T12:00:00');
    vi.setSystemTime(now);
    const oneHourAgo = new Date(now.getTime() - 60 * 60 * 1000).toISOString();
    const result = formatDeadline(oneHourAgo);
    expect(result?.overdue).toBe(true);
    expect(result?.label).toBe('今日');
  });

  it('1時間後の期限は未超過と判定する', () => {
    const now = new Date('2026-07-14T12:00:00');
    vi.setSystemTime(now);
    const oneHourLater = new Date(now.getTime() + 60 * 60 * 1000).toISOString();
    const result = formatDeadline(oneHourLater);
    expect(result?.overdue).toBe(false);
    expect(result?.label).toBe('今日');
  });

  it('日付境界を跨ぐとカレンダー日で超過日数を表示する', () => {
    const now = new Date('2026-07-14T12:00:00');
    vi.setSystemTime(now);
    const yesterday = new Date('2026-07-13T23:59:00').toISOString();
    const result = formatDeadline(yesterday);
    expect(result?.overdue).toBe(true);
    expect(result?.label).toBe('1日超過');
  });

  it('遠い過去は超過を検出する', () => {
    const result = formatDeadline('2020-01-01T00:00:00Z');
    expect(result?.overdue).toBe(true);
  });
});

describe('PRIORITY_CONFIG', () => {
  it('全優先度にラベルがある', () => {
    expect(PRIORITY_CONFIG.High.label).toBe('高');
    expect(PRIORITY_CONFIG.Trivial.label).toBe('些細');
  });
});

describe('isoToLocalDateInput / localDateInputToIso', () => {
  it('日付入力を UTC ISO に変換して相互変換する', () => {
    const date = '2026-07-02';
    const iso = localDateInputToIso(date);
    expect(iso).toBe('2026-07-02T00:00:00.000Z');
    expect(isoToLocalDateInput(iso)).toBe(date);
  });

  it('表示側のタイムゾーンにかかわらず保存されたカレンダー日を保持する', () => {
    expect(isoToLocalDateInput('2026-07-02T00:00:00+14:00')).toBe('2026-07-02');
    expect(isoToLocalDateInput('2026-07-02T00:00:00-11:00')).toBe('2026-07-02');
  });

  it('空値は空文字を返す', () => {
    expect(isoToLocalDateInput(null)).toBe('');
  });
});

describe('formatProgressPct / clampProgressPct', () => {
  it('進捗率を 0-100 にクランプする', () => {
    expect(clampProgressPct(-5)).toBe(0);
    expect(clampProgressPct(150)).toBe(100);
    expect(clampProgressPct(42.6)).toBe(43);
  });

  it('進捗率をパーセント表示する', () => {
    expect(formatProgressPct(30)).toBe('30%');
  });
});

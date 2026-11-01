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

  describe('完了の状態にある task（done）', () => {
    it('期限を過ぎてから完了した物は超過にせず、期限の日付だけを出す', () => {
      vi.setSystemTime(new Date('2026-07-14T12:00:00'));
      const result = formatDeadline('2026-07-10T00:00:00', { done: true });
      expect(result).toEqual({ label: '7月10日', overdue: false });
    });

    it('期限の前に完了した物も、相対の字ではなく期限の日付を出す', () => {
      vi.setSystemTime(new Date('2026-07-14T12:00:00'));
      const result = formatDeadline('2026-07-16T00:00:00', { done: true });
      expect(result).toEqual({ label: '7月16日', overdue: false });
    });

    it('期限の当日に完了した物も超過にしない', () => {
      const now = new Date('2026-07-14T12:00:00');
      vi.setSystemTime(now);
      const oneHourAgo = new Date(now.getTime() - 60 * 60 * 1000).toISOString();
      expect(formatDeadline(oneHourAgo, { done: true })?.overdue).toBe(false);
    });

    it('期限より後の日に完了した物は、遅れた日数を中立で添える', () => {
      vi.setSystemTime(new Date('2026-07-20T12:00:00'));
      const result = formatDeadline('2026-07-10T00:00:00', {
        done: true,
        completedAt: '2026-07-14T09:00:00',
      });
      expect(result).toEqual({ label: '7月10日（4日超過）', overdue: false });
    });

    it('遅れは完了した日から数え、今日が進んでも増えない', () => {
      const options = { done: true, completedAt: '2026-07-14T09:00:00' };
      vi.setSystemTime(new Date('2026-07-15T12:00:00'));
      const first = formatDeadline('2026-07-10T00:00:00', options);
      vi.setSystemTime(new Date('2026-08-30T12:00:00'));
      const later = formatDeadline('2026-07-10T00:00:00', options);
      expect(first?.label).toBe('7月10日（4日超過）');
      expect(later?.label).toBe('7月10日（4日超過）');
    });

    it('期限の前に完了した物は、期限の日付だけを出す', () => {
      vi.setSystemTime(new Date('2026-07-20T12:00:00'));
      const result = formatDeadline('2026-07-10T00:00:00', {
        done: true,
        completedAt: '2026-07-06T09:00:00',
      });
      expect(result).toEqual({ label: '7月10日', overdue: false });
    });

    it('期限の当日の遅い刻に完了した物は、暦の日が同じなので日付だけを出す', () => {
      vi.setSystemTime(new Date('2026-07-20T12:00:00'));
      const result = formatDeadline('2026-07-10T00:00:00', {
        done: true,
        completedAt: '2026-07-10T23:00:00',
      });
      expect(result).toEqual({ label: '7月10日', overdue: false });
    });

    it('completedAt が空なら、今日から数えて埋めず日付だけを出す', () => {
      vi.setSystemTime(new Date('2026-07-20T12:00:00'));
      expect(formatDeadline('2026-07-10T00:00:00', { done: true, completedAt: null })).toEqual({
        label: '7月10日',
        overdue: false,
      });
    });

    it('done が false なら completedAt があっても今までどおり今日から数えて超過を出す', () => {
      vi.setSystemTime(new Date('2026-07-14T12:00:00'));
      expect(
        formatDeadline('2026-07-10T00:00:00', { done: false, completedAt: '2026-07-11T09:00:00' }),
      ).toEqual({ label: '4日超過', overdue: true });
    });

    it('done が false なら今までどおり超過を出す', () => {
      vi.setSystemTime(new Date('2026-07-14T12:00:00'));
      expect(formatDeadline('2026-07-10T00:00:00', { done: false })).toEqual({
        label: '4日超過',
        overdue: true,
      });
    });
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

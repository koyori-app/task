import { describe, expect, it } from 'vitest';
import {
  formatDeadline,
  formatTaskDate,
  PRIORITY_CONFIG,
  taskDetailHref,
  taskSeqKey,
} from '../task-display';

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
  it('期限超過を検出する', () => {
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

import { describe, expect, it } from 'vitest';

import { PRIORITY_CONFIG, type ApiPriority } from '@/lib/task-display';

import { hex, ratio, token } from './design-token';

/**
 * 優先度のアイコンの色の対比。アイコンは文字でない部品なので、地と 3:1 以上が要る（WCAG 1.4.11）。
 * 色は tailwind.css の `--priority-*` に置き、明るい配色で届かない色だけを `[data-theme='light']` で替える。
 * 測り方は palette-text-contrast.test.ts と同じ design-token.ts の token と ratio。
 */
const VARIABLES: Record<ApiPriority, string> = {
  CriticalFire: 'priority-critical-fire',
  Critical: 'priority-critical',
  High: 'priority-high',
  Medium: 'priority-medium',
  Low: 'priority-low',
  Trivial: 'priority-trivial',
};

/** 暗い配色の色。明るい配色を直す前の値で、暗い配色の見た目はこれから変えない。 */
const DARK_COLORS: Record<ApiPriority, string> = {
  CriticalFire: '#dc2626',
  Critical: '#ef4444',
  High: '#f97316',
  Medium: '#eab308',
  Low: '#6b7280',
  Trivial: '#9ca3af',
};

const PRIORITIES = Object.keys(VARIABLES) as ApiPriority[];

describe('優先度のアイコンの色', () => {
  it('色は配色で替わる CSS 変数で渡す', () => {
    for (const priority of PRIORITIES) {
      expect(PRIORITY_CONFIG[priority].color).toBe(`var(--${VARIABLES[priority]})`);
    }
  });

  it.each(PRIORITIES)('明るい配色で %s は白地と bg-muted の上で 3:1 以上', (priority) => {
    const color = token('light', VARIABLES[priority]);
    expect(ratio(color, token('light', 'background'))).toBeGreaterThanOrEqual(3);
    // 行の hover・選択の地
    expect(ratio(color, token('light', 'muted'))).toBeGreaterThanOrEqual(3);
  });

  it('暗い配色の色は変えない', () => {
    for (const priority of PRIORITIES) {
      expect(token('dark', VARIABLES[priority])).toEqual(hex(DARK_COLORS[priority]));
    }
  });

  it('検査が効いている: 暗い配色の「中」は白地で 3:1 に届かない', () => {
    expect(ratio(hex('#eab308'), token('light', 'background'))).toBeLessThan(3);
  });
});

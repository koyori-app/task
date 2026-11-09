import { describe, expect, it } from 'vitest';

import { chartHeight, chartProps, chartRows, GROUP_H, HEADER_H, STANDARD_ROW_H } from '../chart';
import type { GanttLayout } from '../layout';

const layout: GanttLayout = {
  groups: [],
  tasks: [{ id: 'a', title: 'a', progress_pct: 0, start: '2026-11-09', end: '2026-11-10' }],
  rows: [{ kind: 'group' }, { kind: 'task', task_id: 'a' }],
  skipped: 0,
};

describe('chartRows と chartHeight', () => {
  it('広い画面は課題の行を標準の高さのままにし、群れの見出しに高さを付ける', () => {
    expect(chartRows(layout, false)).toEqual([
      { kind: 'group', height: GROUP_H },
      { kind: 'task', task_id: 'a' },
    ]);
    expect(chartHeight(chartRows(layout, false), false)).toBe(HEADER_H + GROUP_H + STANDARD_ROW_H);
  });

  it('狭い画面は課題の行だけを高くし、標準の高さは変えない（棒の上に題の二行の隙を作る）', () => {
    expect(chartRows(layout, true)[1]).toEqual({ kind: 'task', task_id: 'a', height: 72 });
    const props = chartProps({
      layout,
      today: '2026-11-09',
      from: '2026-11-09',
      to: '2026-11-15',
      scale: 'day',
      pxPerDay: 46,
      compact: true,
      progressLine: false,
      backend: 'svg',
    });
    expect(props.rowHeight).toBe(STANDARD_ROW_H);
    expect(props.barAlign).toBe('bottom');
    expect(props.progressLine).toBe('hidden');
    expect(chartHeight(props.rows, true)).toBe(HEADER_H + GROUP_H + 72);
  });
});

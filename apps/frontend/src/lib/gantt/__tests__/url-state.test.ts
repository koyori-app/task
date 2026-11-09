import { describe, expect, it } from 'vitest';

import { applyGanttUrlState, parseGanttUrlState, type GanttUrlState } from '../url-state';

const TODAY = '2026-11-11';

function state(over: Partial<GanttUrlState> = {}): GanttUrlState {
  return {
    from: '2026-11-09',
    memberIds: [],
    scale: 'day',
    groupBy: 'project',
    progressLine: null,
    ...over,
  };
}

function urlOf(s: GanttUrlState, base = 'https://example.test/t/gantt'): URL {
  return applyGanttUrlState(new URL(base), s, TODAY);
}

describe('parseGanttUrlState', () => {
  it('何も無ければ今週・日の目盛り・プロジェクトの群れ・稲妻線は覚えた値に任せる', () => {
    expect(parseGanttUrlState(undefined, TODAY)).toEqual(state());
  });

  it('目盛り・群れ・稲妻線を読み、知らぬ値は既定へ倒す', () => {
    expect(
      parseGanttUrlState(
        { scale: ['month'], group: ['assignee'], line: ['on'], from: ['2026-11-18'] },
        TODAY,
      ),
    ).toEqual(
      state({ from: '2026-11-16', scale: 'month', groupBy: 'assignee', progressLine: true }),
    );
    expect(parseGanttUrlState({ line: ['off'] }, TODAY).progressLine).toBe(false);
    expect(parseGanttUrlState({ scale: ['year'], group: ['label'], line: ['1'] }, TODAY)).toEqual(
      state(),
    );
  });
});

describe('applyGanttUrlState', () => {
  it('既定の値は書かず、URL を短く保つ', () => {
    expect(urlOf(state()).search).toBe('');
  });

  it('稲妻線は点けても切っても明に書く（覚えた値と違う者にも伝わる）', () => {
    expect(urlOf(state({ progressLine: true })).searchParams.get('line')).toBe('on');
    expect(urlOf(state({ progressLine: false })).searchParams.get('line')).toBe('off');
  });

  it('書いた URL を読めば同じ状態に戻る', () => {
    const s = state({
      from: '2026-11-23',
      memberIds: ['u-1', 'unassigned'],
      scale: 'week',
      groupBy: 'assignee',
      progressLine: true,
    });
    const url = urlOf(s);
    const searchAll: Record<string, string[]> = {};
    for (const [key, value] of url.searchParams) (searchAll[key] ??= []).push(value);
    expect(parseGanttUrlState(searchAll, TODAY)).toEqual(s);
  });

  it('扱わないキーは触らない', () => {
    expect(urlOf(state({ scale: 'month' }), 'https://example.test/t/gantt?q=1').search).toBe(
      '?q=1&scale=month',
    );
  });
});

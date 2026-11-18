import { describe, expect, it } from 'vitest';

import {
  HOME_DASHBOARD_TABS,
  applyHomeDashboardUrlState,
  parseHomeDashboardUrlState,
} from '../home-dashboard-url-state';

const BASE = 'https://app.example.com/acme';

describe('parseHomeDashboardUrlState', () => {
  it('何も無ければ今日のタブの 1 ページ目', () => {
    expect(parseHomeDashboardUrlState(undefined)).toEqual({ tab: 'today', page: 1 });
    expect(parseHomeDashboardUrlState({})).toEqual({ tab: 'today', page: 1 });
  });

  it('tab と page を読む', () => {
    expect(parseHomeDashboardUrlState({ tab: 'overdue', page: '3' })).toEqual({
      tab: 'overdue',
      page: 3,
    });
  });

  it('知らない tab は今日、正の整数でない page は 1', () => {
    expect(parseHomeDashboardUrlState({ tab: 'no_due_date' }).tab).toBe('today');
    for (const page of ['0', '-1', '1.5', 'x', '']) {
      expect(parseHomeDashboardUrlState({ page }).page).toBe(1);
    }
  });
});

describe('applyHomeDashboardUrlState', () => {
  it('既定の値は書かない', () => {
    const url = applyHomeDashboardUrlState(new URL(`${BASE}?tab=week&page=2`), {
      tab: 'today',
      page: 1,
    });
    expect(url.search).toBe('');
  });

  it('書いて読めば同じ状態になる', () => {
    for (const tab of HOME_DASHBOARD_TABS) {
      for (const page of [1, 2, 7]) {
        const url = applyHomeDashboardUrlState(new URL(BASE), { tab, page });
        expect(parseHomeDashboardUrlState(Object.fromEntries(url.searchParams))).toEqual({
          tab,
          page,
        });
      }
    }
  });

  it('扱わないキーは触らない', () => {
    const url = applyHomeDashboardUrlState(new URL(`${BASE}?keep=1`), { tab: 'all', page: 2 });
    expect(url.searchParams.get('keep')).toBe('1');
    expect(url.searchParams.get('tab')).toBe('all');
    expect(url.searchParams.get('page')).toBe('2');
  });
});

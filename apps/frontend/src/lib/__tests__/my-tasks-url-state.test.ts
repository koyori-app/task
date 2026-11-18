import { describe, expect, it } from 'vitest';

import {
  MY_TASKS_TABS,
  applyMyTasksUrlState,
  myTasksEmptyMessage,
  parseMyTasksUrlState,
} from '../my-tasks-url-state';

const BASE = 'https://app.example.com/acme/my-tasks';

describe('parseMyTasksUrlState', () => {
  it('何も無ければ今日のタブで 50 件', () => {
    expect(parseMyTasksUrlState(undefined)).toEqual({ tab: 'today', shown: 50 });
    expect(parseMyTasksUrlState({})).toEqual({ tab: 'today', shown: 50 });
  });

  it('tab と shown を読む', () => {
    expect(parseMyTasksUrlState({ tab: 'week', shown: '100' })).toEqual({
      tab: 'week',
      shown: 100,
    });
  });

  it('知らない tab は今日', () => {
    expect(parseMyTasksUrlState({ tab: 'tomorrow' }).tab).toBe('today');
  });

  it('shown は 50 の倍数へ切り上げ、200 で頭打ち、不正な値は 50', () => {
    expect(parseMyTasksUrlState({ shown: '51' }).shown).toBe(100);
    expect(parseMyTasksUrlState({ shown: '1' }).shown).toBe(50);
    expect(parseMyTasksUrlState({ shown: '0' }).shown).toBe(50);
    expect(parseMyTasksUrlState({ shown: '250' }).shown).toBe(200);
    expect(parseMyTasksUrlState({ shown: '-100' }).shown).toBe(50);
    expect(parseMyTasksUrlState({ shown: '1.5' }).shown).toBe(50);
    expect(parseMyTasksUrlState({ shown: 'x' }).shown).toBe(50);
  });
});

describe('applyMyTasksUrlState', () => {
  it('既定の値は書かない', () => {
    const url = applyMyTasksUrlState(new URL(`${BASE}?tab=week&shown=100`), {
      tab: 'today',
      shown: 50,
    });
    expect(url.search).toBe('');
  });

  it('tab と shown を書き、shown は 200 で頭打ち', () => {
    const url = applyMyTasksUrlState(new URL(BASE), { tab: 'overdue', shown: 250 });
    expect(url.searchParams.get('tab')).toBe('overdue');
    expect(url.searchParams.get('shown')).toBe('200');
  });

  it('書いて読めば同じ状態になる', () => {
    for (const tab of MY_TASKS_TABS) {
      for (const shown of [50, 100, 150, 200]) {
        const url = applyMyTasksUrlState(new URL(BASE), { tab, shown });
        expect(parseMyTasksUrlState(Object.fromEntries(url.searchParams))).toEqual({ tab, shown });
      }
    }
  });

  it('扱わないキーは触らない', () => {
    const url = applyMyTasksUrlState(new URL(`${BASE}?keep=1`), { tab: 'all', shown: 100 });
    expect(url.searchParams.get('keep')).toBe('1');
  });
});

describe('myTasksEmptyMessage', () => {
  it('タブごとに別の文言を返し、今日は「割り当てられたタスクは0件です」ではない', () => {
    const messages = MY_TASKS_TABS.map(myTasksEmptyMessage);
    expect(new Set(messages).size).toBe(MY_TASKS_TABS.length);
    expect(myTasksEmptyMessage('today')).not.toBe('割り当てられたタスクは0件です');
    expect(myTasksEmptyMessage('all')).toBe('割り当てられたタスクは0件です');
  });
});

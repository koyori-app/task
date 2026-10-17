import { describe, expect, it } from 'vitest';

import { applyCalendarUrlState, parseCalendarUrlState, shiftFrom } from '../url-state';

const TODAY = '2026-10-16'; // 金
const BASE = 'https://app.example.com/acme/calendar';

describe('parseCalendarUrlState', () => {
  it('from が無ければ今日を含む週の月曜', () => {
    expect(parseCalendarUrlState(undefined, TODAY)).toEqual({
      from: '2026-10-12',
      memberIds: [],
      projectIds: [],
    });
  });

  it('途中の曜日は月曜に揃え、不正な日付は今週に倒す', () => {
    expect(parseCalendarUrlState({ from: ['2026-11-04'] }, TODAY).from).toBe('2026-11-02');
    expect(parseCalendarUrlState({ from: ['2026-02-30'] }, TODAY).from).toBe('2026-10-12');
    expect(parseCalendarUrlState({ from: ['x'] }, TODAY).from).toBe('2026-10-12');
  });

  it('member / project は複数を読み、重複と空を落とす', () => {
    const state = parseCalendarUrlState({ member: ['u1', 'u2', 'u1', ''], project: ['p1'] }, TODAY);
    expect(state.memberIds).toEqual(['u1', 'u2']);
    expect(state.projectIds).toEqual(['p1']);
  });
});

describe('shiftFrom', () => {
  it('前後の移動で 14 日ずれる', () => {
    expect(shiftFrom('2026-10-12', 1)).toBe('2026-10-26');
    expect(shiftFrom('2026-10-12', -1)).toBe('2026-09-28');
  });
});

describe('applyCalendarUrlState', () => {
  it('今週の from は落とし、それ以外は書く。複数値は繰り返しで持つ', () => {
    const url = applyCalendarUrlState(
      new URL(`${BASE}?from=2026-09-28&keep=1`),
      { from: '2026-10-12', memberIds: ['u1', 'u2'], projectIds: [] },
      TODAY,
    );
    expect(url.searchParams.has('from')).toBe(false);
    expect(url.searchParams.getAll('member')).toEqual(['u1', 'u2']);
    expect(url.searchParams.has('project')).toBe(false);
    // 扱わないキーは触らない
    expect(url.searchParams.get('keep')).toBe('1');

    const next = applyCalendarUrlState(
      url,
      { from: '2026-10-26', memberIds: [], projectIds: ['p1'] },
      TODAY,
    );
    expect(next.searchParams.get('from')).toBe('2026-10-26');
    expect(next.searchParams.has('member')).toBe(false);
    expect(next.searchParams.getAll('project')).toEqual(['p1']);
  });
});

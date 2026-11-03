import { describe, expect, it } from 'vitest';

import { taskSpan } from '@/lib/calendar/timeline';

import { UNASSIGNED_MEMBER } from '@/lib/calendar/filters';
import {
  applyCalendarUrlState,
  parseCalendarUrlState,
  type CalendarUrlState,
} from '@/lib/calendar/url-state';

import { toGanttBars, toGanttBarsFor, type GanttSource } from '../tasks';

function task(over: Partial<GanttSource> & { id: string }): GanttSource {
  return { title: `課題 ${over.id}`, progress_pct: 0, ...over };
}

describe('toGanttBars', () => {
  describe('棒の端はカレンダーの taskSpan と一致する', () => {
    // 期待の日付は書かず、同じ課題を taskSpan に通した結果と比べる
    const cases: [string, Partial<GanttSource>][] = [
      [
        'soft が hard より後',
        {
          planned_start_on: '2026-10-10',
          soft_deadline: '2026-10-20T03:00:00Z',
          hard_deadline: '2026-10-14T03:00:00Z',
        },
      ],
      [
        'hard が soft より後',
        {
          planned_start_on: '2026-10-10',
          soft_deadline: '2026-10-14T03:00:00Z',
          hard_deadline: '2026-10-20T03:00:00Z',
        },
      ],
      ['始まりだけ', { planned_start_on: '2026-10-14' }],
      ['hard だけ', { hard_deadline: '2026-10-20T03:00:00Z' }],
      [
        '始まりが締切より後',
        { planned_start_on: '2026-10-25', soft_deadline: '2026-10-14T03:00:00Z' },
      ],
    ];

    it.each(cases)('%s', (_name, over) => {
      for (const zone of ['UTC', 'Asia/Tokyo', 'America/Los_Angeles']) {
        const source = task({ id: 'a', ...over });
        const span = taskSpan(source, zone)!;
        const { bars } = toGanttBars([source], zone);
        expect(bars).toHaveLength(1);
        expect([bars[0]!.start, bars[0]!.end]).toEqual([span.start, span.end]);
      }
    });
  });

  it('期間の無い課題は除き、除いた数を返す', () => {
    const { bars, skipped } = toGanttBars(
      [
        task({ id: 'a', planned_start_on: '2026-10-12' }),
        task({ id: 'b' }),
        task({ id: 'c', soft_deadline: null, hard_deadline: null, planned_start_on: null }),
      ],
      'UTC',
    );
    expect(bars.map((bar) => bar.id)).toEqual(['a']);
    expect(skipped).toBe(2);
  });

  it('課題が無ければ空', () => {
    expect(toGanttBars([], 'UTC')).toEqual({ bars: [], skipped: 0 });
  });

  it('題と進捗はそのまま渡す', () => {
    const { bars } = toGanttBars(
      [task({ id: 'a', title: '設計', progress_pct: 40, planned_start_on: '2026-10-12' })],
      'UTC',
    );
    expect(bars[0]).toEqual({
      id: 'a',
      title: '設計',
      progress_pct: 40,
      start: '2026-10-12',
      end: '2026-10-12',
    });
  });

  describe('刻の帯', () => {
    // UTC では 10-16、東京では 10-17 になる締切
    const deadline = '2026-10-16T15:00:00Z';

    it('同じ締切でも帯が違えば別の日で終わり、カレンダーと同じ日になる', () => {
      for (const zone of ['UTC', 'Asia/Tokyo', 'America/Los_Angeles']) {
        const source = task({ id: 'a', planned_start_on: '2026-10-12', soft_deadline: deadline });
        const { bars } = toGanttBars([source], zone);
        expect(bars[0]!.end).toBe(taskSpan(source, zone)!.end);
      }
      const at = (zone: string) =>
        toGanttBars(
          [task({ id: 'a', planned_start_on: '2026-10-12', soft_deadline: deadline })],
          zone,
        ).bars[0]!.end;
      expect(at('UTC')).toBe('2026-10-16');
      expect(at('Asia/Tokyo')).toBe('2026-10-17');
    });

    it('渡す字は日付だけで、時刻や帯を含まない', () => {
      const { bars } = toGanttBars(
        [task({ id: 'a', planned_start_on: '2026-10-12', soft_deadline: deadline })],
        'Asia/Tokyo',
      );
      expect(bars[0]!.start).toMatch(/^\d{4}-\d{2}-\d{2}$/);
      expect(bars[0]!.end).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    });
  });
});

describe('toGanttBarsFor', () => {
  type Row = GanttSource & { assignees: { user_id: string }[] };
  const row = (id: string, userIds: string[], over: Partial<GanttSource> = {}): Row => ({
    ...task({ id, planned_start_on: '2026-10-12', ...over }),
    assignees: userIds.map((user_id) => ({ user_id })),
  });
  const rows: Row[] = [
    row('a', ['alice']),
    row('b', ['bob']),
    row('ab', ['alice', 'bob']),
    row('none', []),
    // 期間が無い課題（担当ごとに数が変わることを見る）
    row('a-nospan', ['alice'], { planned_start_on: null }),
    row('none-nospan', [], { planned_start_on: null }),
    row('b-nospan', ['bob'], { planned_start_on: null }),
  ];
  const ids = (memberIds: string[]) =>
    toGanttBarsFor(rows, memberIds, 'UTC').bars.map((bar) => bar.id);

  it('何も選ばなければ全件で、期間の無い数は全件の数', () => {
    expect(ids([])).toEqual(['a', 'b', 'ab', 'none']);
    expect(toGanttBarsFor(rows, [], 'UTC').skipped).toBe(3);
  });

  it('一人選べば、その者の課題だけが出る', () => {
    expect(ids(['alice'])).toEqual(['a', 'ab']);
  });

  it('二人選べば、二人の課題が一つの並びに合わさる', () => {
    const { bars } = toGanttBarsFor(rows, ['alice', 'bob'], 'UTC');
    expect(bars.map((bar) => bar.id)).toEqual(['a', 'b', 'ab']);
  });

  it('二人が担当する課題は、どちらを選んでも出る', () => {
    expect(ids(['alice'])).toContain('ab');
    expect(ids(['bob'])).toContain('ab');
  });

  it('担当なしを選べば、担当の無い課題だけが出る', () => {
    expect(ids([UNASSIGNED_MEMBER])).toEqual(['none']);
  });

  it('担当なしと一人を合わせて選べる', () => {
    expect(ids([UNASSIGNED_MEMBER, 'bob'])).toEqual(['b', 'ab', 'none']);
  });

  it('誰かを選んだ時、担当の無い課題は出ない', () => {
    expect(ids(['alice'])).not.toContain('none');
  });

  it('期間の無い課題の数は、絞った後の数', () => {
    expect(toGanttBarsFor(rows, ['alice'], 'UTC').skipped).toBe(1);
    expect(toGanttBarsFor(rows, ['alice', 'bob'], 'UTC').skipped).toBe(2);
    expect(toGanttBarsFor(rows, [UNASSIGNED_MEMBER], 'UTC').skipped).toBe(1);
    expect(toGanttBarsFor(rows, ['nobody'], 'UTC')).toEqual({ bars: [], skipped: 0 });
  });
});

describe('担当者を URL に残す', () => {
  const today = '2026-10-14';
  const state = (memberIds: string[]): CalendarUrlState => ({
    from: '2026-10-12',
    memberIds,
    projectIds: [],
    selected: null,
  });
  const searchAll = (url: URL) => {
    const out: Record<string, string[]> = {};
    for (const [key, value] of url.searchParams) (out[key] ??= []).push(value);
    return out;
  };

  it('member を並べて書き、読み戻すと同じ者が選ばれている', () => {
    const url = applyCalendarUrlState(
      new URL('https://example.test/acme/gantt'),
      state(['u1', UNASSIGNED_MEMBER, 'u2']),
      today,
    );
    expect(url.search).toBe('?member=u1&member=unassigned&member=u2');
    expect(parseCalendarUrlState(searchAll(url), today).memberIds).toEqual([
      'u1',
      UNASSIGNED_MEMBER,
      'u2',
    ]);
  });

  it('全部外せば member の項が URL から消える', () => {
    const url = applyCalendarUrlState(
      new URL('https://example.test/acme/gantt?member=u1'),
      state([]),
      today,
    );
    expect(url.search).toBe('');
  });
});

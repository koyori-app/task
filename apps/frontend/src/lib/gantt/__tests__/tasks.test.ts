import { describe, expect, it } from 'vitest';

import { taskSpan } from '@/lib/calendar/timeline';

import { toGanttBars, type GanttSource } from '../tasks';

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

import { describe, expect, it } from 'vitest';

import { dailyLoadByMember, isOverCapacity, weekdaysBetween, type LoadSource } from '../load';

const ME = 'user-1';
const OTHER = 'user-2';

function task(overrides: Partial<LoadSource> = {}): LoadSource {
  return {
    planned_start_on: '2026-10-12', // 月
    soft_deadline: '2026-10-16T00:00:00Z', // 金
    estimated_minutes: 600,
    status: { is_done_state: false },
    assignees: [{ user_id: ME, role: 'primary' }],
    ...overrides,
  };
}

describe('weekdaysBetween', () => {
  it('土日を除いた平日を両端含みで数える', () => {
    // 金〜翌火 = 金・月・火
    expect(weekdaysBetween('2026-10-16', '2026-10-20')).toEqual([
      '2026-10-16',
      '2026-10-19',
      '2026-10-20',
    ]);
  });

  it('土日だけの期間は 0 日', () => {
    expect(weekdaysBetween('2026-10-17', '2026-10-18')).toEqual([]);
  });
});

describe('dailyLoadByMember', () => {
  it('見積を平日数で割って各平日に載せる', () => {
    const load = dailyLoadByMember([task()], 'UTC').get(ME)!;
    expect([...load].map(([date, day]) => [date, day.hours])).toEqual([
      ['2026-10-12', 2],
      ['2026-10-13', 2],
      ['2026-10-14', 2],
      ['2026-10-15', 2],
      ['2026-10-16', 2],
    ]);
  });

  it('土日には載せず、平日数だけで割る', () => {
    // 金〜翌月 = 平日 2 日、8h → 4h/日
    const load = dailyLoadByMember(
      [
        task({
          planned_start_on: '2026-10-16',
          soft_deadline: '2026-10-19T00:00:00Z',
          estimated_minutes: 480,
        }),
      ],
      'UTC',
    ).get(ME)!;
    expect(load.get('2026-10-16')?.hours).toBe(4);
    expect(load.get('2026-10-19')?.hours).toBe(4);
    expect(load.has('2026-10-17')).toBe(false);
    expect(load.has('2026-10-18')).toBe(false);
  });

  it('同じ日の見積を足し合わせ、上限 6h を超えたら超過。内訳にタスクごとの時間を残す', () => {
    const first = task();
    const second = task({ estimated_minutes: 1500 });
    const day = dailyLoadByMember([first, second], 'UTC').get(ME)!.get('2026-10-14')!;
    // 2h + 5h = 7h
    expect(day.hours).toBe(7);
    expect(isOverCapacity(day.hours)).toBe(true);
    expect(day.items).toEqual([
      { task: first, hours: 2 },
      { task: second, hours: 5 },
    ]);
  });

  it.each([
    [6, false],
    [6.01, true],
  ])('%sh は超過か: %s（上限ちょうどは超過にしない）', (hours, over) => {
    expect(isOverCapacity(hours)).toBe(over);
  });

  it('開始予定日・Soft 締切・見積の無いタスク、完了タスクは数えない', () => {
    const load = dailyLoadByMember(
      [
        task({ planned_start_on: null }),
        task({ soft_deadline: null }),
        task({ estimated_minutes: null }),
        task({ status: { is_done_state: true } }),
      ],
      'UTC',
    );
    expect(load.size).toBe(0);
  });

  it('主担当だけに載せ、サブ担当には載せない', () => {
    const load = dailyLoadByMember(
      [
        task({
          assignees: [
            { user_id: ME, role: 'primary' },
            { user_id: OTHER, role: 'assignee' },
          ],
        }),
      ],
      'UTC',
    );
    expect(load.has(ME)).toBe(true);
    expect(load.has(OTHER)).toBe(false);
  });

  it('Soft 締切はタイムゾーンで日付に落としてから平日を数える', () => {
    // UTC 木曜 15:00 = 東京の金曜 0:00 → 東京では月〜金の 5 日
    const tokyo = dailyLoadByMember(
      [task({ soft_deadline: '2026-10-15T15:00:00Z' })],
      'Asia/Tokyo',
    ).get(ME)!;
    // UTC なら木曜まで = 4 日
    const utc = dailyLoadByMember([task({ soft_deadline: '2026-10-15T15:00:00Z' })], 'UTC').get(
      ME,
    )!;
    expect(tokyo.size).toBe(5);
    expect(utc.size).toBe(4);
  });
});

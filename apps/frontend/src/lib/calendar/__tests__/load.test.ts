import { describe, expect, it } from 'vitest';

import { dailyLoadByMember, isOverCapacity, weekdaysBetween, type LoadSource } from '../load';

const ME = 'user-1';
const OTHER = 'user-2';
/** 月曜。既定のタスク（月〜金）の初日 */
const MONDAY = '2026-10-12';

function task(overrides: Partial<LoadSource> = {}): LoadSource {
  return {
    planned_start_on: '2026-10-12', // 月
    soft_deadline: '2026-10-16T00:00:00Z', // 金
    estimated_minutes: 600,
    logged_minutes: 0,
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
  const hoursOf = (load: Map<string, { hours: number }> | undefined) =>
    [...(load ?? [])].map(([date, day]) => [date, day.hours]);

  it('残工数を平日数で割って各平日に載せる', () => {
    const load = dailyLoadByMember([task()], 'UTC', MONDAY).get(ME)!;
    expect(hoursOf(load)).toEqual([
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
      MONDAY,
    ).get(ME)!;
    expect(load.get('2026-10-16')?.hours).toBe(4);
    expect(load.get('2026-10-19')?.hours).toBe(4);
    expect(load.has('2026-10-17')).toBe(false);
    expect(load.has('2026-10-18')).toBe(false);
  });

  it('同じ日の残工数を足し合わせ、上限 6h を超えたら超過。内訳にタスクごとの時間を残す', () => {
    const first = task();
    const second = task({ estimated_minutes: 1500 });
    const day = dailyLoadByMember([first, second], 'UTC', MONDAY).get(ME)!.get('2026-10-14')!;
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

  // 仕様 §3.5 の現行からの変更点
  it('積む量: 見積ではなく残工数（見積 − 実績）を積む', () => {
    // 10h − 5h = 5h を 5 日で 1h/日
    const load = dailyLoadByMember([task({ logged_minutes: 300 })], 'UTC', MONDAY).get(ME);
    expect(load?.get('2026-10-12')?.hours).toBe(1);
    expect(load?.size).toBe(5);
  });

  it('期間・過去の日: 今日〜Soft 締切に切り詰め、今日より前の日には積まない', () => {
    // 水曜が今日なら水〜金の 3 日で割る
    const load = dailyLoadByMember([task({ estimated_minutes: 540 })], 'UTC', '2026-10-14').get(ME);
    expect(hoursOf(load)).toEqual([
      ['2026-10-14', 3],
      ['2026-10-15', 3],
      ['2026-10-16', 3],
    ]);
  });

  it('締切超過: 平日が残っていなければ今日 1 日に全部積む', () => {
    const load = dailyLoadByMember([task()], 'UTC', '2026-10-20').get(ME);
    expect(hoursOf(load)).toEqual([['2026-10-20', 10]]);
  });

  it('始点〜終点が土日だけなら直前の平日 1 日に全部積む', () => {
    // 2 週後の土日でも今日には積まない（表示期間より後のタスクは取得しないので、今日に積むと漏れる）
    const load = dailyLoadByMember(
      [task({ planned_start_on: '2026-10-24', soft_deadline: '2026-10-25T00:00:00Z' })],
      'UTC',
      MONDAY,
    ).get(ME);
    expect(hoursOf(load)).toEqual([['2026-10-23', 10]]);
  });

  it('土日だけの期間の直前の平日が今日より前なら今日に積む', () => {
    // 今日は土曜。直前の平日（金曜）は過去
    const load = dailyLoadByMember(
      [task({ planned_start_on: '2026-10-17', soft_deadline: '2026-10-18T00:00:00Z' })],
      'UTC',
      '2026-10-17',
    ).get(ME);
    expect(hoursOf(load)).toEqual([['2026-10-17', 10]]);
  });

  it('担当者: 全担当者で均等割りし、primary と assignee を区別しない', () => {
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
      MONDAY,
    );
    // 2h/日を 2 人で 1h ずつ
    expect(load.get(ME)?.get('2026-10-12')?.hours).toBe(1);
    expect(load.get(OTHER)?.get('2026-10-12')?.hours).toBe(1);
    expect(hoursOf(load.get(ME))).toEqual(hoursOf(load.get(OTHER)));
  });

  it('開始予定日が Soft 締切より後なら両端を入れ替えて積む', () => {
    const load = dailyLoadByMember(
      [task({ planned_start_on: '2026-10-16', soft_deadline: '2026-10-12T00:00:00Z' })],
      'UTC',
      MONDAY,
    ).get(ME);
    expect(load?.size).toBe(5);
    expect(load?.get('2026-10-12')?.hours).toBe(2);
  });

  it('残工数 0 以下・開始予定日・Soft 締切・見積・担当者の無いタスク、完了タスクは積まない', () => {
    const load = dailyLoadByMember(
      [
        task({ logged_minutes: 600 }),
        task({ logged_minutes: 700 }),
        task({ planned_start_on: null }),
        task({ soft_deadline: null }),
        task({ estimated_minutes: null }),
        task({ assignees: [] }),
        task({ status: { is_done_state: true } }),
      ],
      'UTC',
      MONDAY,
    );
    expect(load.size).toBe(0);
  });

  it('残工数が 1 分なら積む', () => {
    const load = dailyLoadByMember([task({ logged_minutes: 599 })], 'UTC', MONDAY);
    expect(load.get(ME)?.size).toBe(5);
  });

  it('Soft 締切はタイムゾーンで日付に落としてから平日を数える', () => {
    // UTC 木曜 15:00 = 東京の金曜 0:00 → 東京では月〜金の 5 日
    const tokyo = dailyLoadByMember(
      [task({ soft_deadline: '2026-10-15T15:00:00Z' })],
      'Asia/Tokyo',
      MONDAY,
    ).get(ME)!;
    // UTC なら木曜まで = 4 日
    const utc = dailyLoadByMember(
      [task({ soft_deadline: '2026-10-15T15:00:00Z' })],
      'UTC',
      MONDAY,
    ).get(ME)!;
    expect(tokyo.size).toBe(5);
    expect(utc.size).toBe(4);
  });
});

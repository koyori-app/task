import { describe, expect, it } from 'vitest';

import { addDays, clipSpan, dateInZone, isDateString, mondayOf, pack, taskSpan } from '../timeline';

describe('日付の計算', () => {
  it('月曜始まりの週の月曜に揃える', () => {
    expect(mondayOf('2026-10-12')).toBe('2026-10-12'); // 月
    expect(mondayOf('2026-10-16')).toBe('2026-10-12'); // 金
    expect(mondayOf('2026-10-18')).toBe('2026-10-12'); // 日は前の月曜
    expect(mondayOf('2026-11-01')).toBe('2026-10-26'); // 月をまたぐ
  });

  it('月末をまたいで足し引きする', () => {
    expect(addDays('2026-10-31', 1)).toBe('2026-11-01');
    expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
  });

  it.each([
    ['2026-10-16', true],
    ['2026-02-30', false],
    ['2026-1-1', false],
    ['abc', false],
  ])('%s を日付として受け付けるか: %s', (value, ok) => {
    expect(isDateString(value)).toBe(ok);
  });

  it('時刻はタイムゾーンで日付に落とす（境界で日付が変わる）', () => {
    // UTC 15:00 は東京では翌日 0:00、UTC ではその日
    expect(dateInZone('2026-10-16T15:00:00Z', 'Asia/Tokyo')).toBe('2026-10-17');
    expect(dateInZone('2026-10-16T14:59:59Z', 'Asia/Tokyo')).toBe('2026-10-16');
    expect(dateInZone('2026-10-16T15:00:00Z', 'UTC')).toBe('2026-10-16');
    // 西側では前日になる
    expect(dateInZone('2026-10-16T03:00:00Z', 'America/Los_Angeles')).toBe('2026-10-15');
  });
});

describe('taskSpan', () => {
  it('開始予定日だけなら 1 日の帯', () => {
    expect(taskSpan({ planned_start_on: '2026-10-14' }, 'UTC')).toEqual({
      start: '2026-10-14',
      end: '2026-10-14',
    });
  });

  it('Soft 締切だけなら締切日の 1 日（点）', () => {
    expect(taskSpan({ soft_deadline: '2026-10-20T00:00:00Z' }, 'UTC')).toEqual({
      start: '2026-10-20',
      end: '2026-10-20',
    });
  });

  it('両方あれば開始予定日〜Soft 締切。Hard より Soft を優先する', () => {
    expect(
      taskSpan(
        {
          planned_start_on: '2026-10-14',
          soft_deadline: '2026-10-20T00:00:00Z',
          hard_deadline: '2026-10-30T00:00:00Z',
        },
        'UTC',
      ),
    ).toEqual({ start: '2026-10-14', end: '2026-10-20' });
  });

  it('Soft が無ければ Hard を終点にする', () => {
    expect(
      taskSpan({ planned_start_on: '2026-10-14', hard_deadline: '2026-10-22T00:00:00Z' }, 'UTC'),
    ).toEqual({ start: '2026-10-14', end: '2026-10-22' });
  });

  it('締切はタイムゾーンで日付に落とす', () => {
    expect(taskSpan({ soft_deadline: '2026-10-20T15:00:00Z' }, 'Asia/Tokyo')?.end).toBe(
      '2026-10-21',
    );
  });

  it('日付が何も無ければ描かない', () => {
    expect(taskSpan({}, 'UTC')).toBeNull();
  });
});

describe('clipSpan', () => {
  const from = '2026-10-12';

  it('範囲の両端にちょうど一致すれば切り詰めない', () => {
    expect(clipSpan({ start: '2026-10-12', end: '2026-10-25' }, from, 14)).toEqual({
      startCol: 0,
      endCol: 13,
      clippedStart: false,
      clippedEnd: false,
    });
  });

  it('1 日はみ出す分は端で切り詰めて印を付ける', () => {
    expect(clipSpan({ start: '2026-10-11', end: '2026-10-26' }, from, 14)).toEqual({
      startCol: 0,
      endCol: 13,
      clippedStart: true,
      clippedEnd: true,
    });
  });

  it('完全に外なら描かない（1 日外れも含む）', () => {
    expect(clipSpan({ start: '2026-10-01', end: '2026-10-11' }, from, 14)).toBeNull();
    expect(clipSpan({ start: '2026-10-26', end: '2026-10-30' }, from, 14)).toBeNull();
  });
});

describe('pack', () => {
  it('重ならない帯は同じ段に詰め、重なる帯は段を分ける', () => {
    const lanes = pack([
      { id: 'a', start: 0, end: 3 },
      { id: 'b', start: 2, end: 5 },
      { id: 'c', start: 4, end: 6 },
      { id: 'd', start: 6, end: 6 },
    ]);
    expect(lanes.map((lane) => lane.map((item) => item.id))).toEqual([
      ['a', 'c'],
      ['b', 'd'],
    ]);
  });

  it('終点と始点が同じ日は重なりとして扱う', () => {
    const lanes = pack([
      { id: 'a', start: '2026-10-12', end: '2026-10-14' },
      { id: 'b', start: '2026-10-14', end: '2026-10-15' },
    ]);
    expect(lanes).toHaveLength(2);
  });

  it('入力順に依らず始点順で詰める', () => {
    const lanes = pack([
      { id: 'late', start: 5, end: 6 },
      { id: 'early', start: 0, end: 1 },
    ]);
    expect(lanes.map((lane) => lane.map((item) => item.id))).toEqual([['early', 'late']]);
  });

  it('空なら段も無い', () => {
    expect(pack([])).toEqual([]);
  });
});

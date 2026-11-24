import { describe, expect, it } from 'vitest';
import { buildBlockingChart, type BlockingChartTask } from '@/lib/task-blocking-gantt';

const statuses = [
  { id: 'todo', is_done_state: false },
  { id: 'done', is_done_state: true },
];
const today = '2026-11-16';

function task(seq: number, overrides: Partial<BlockingChartTask> = {}): BlockingChartTask {
  return {
    id: `t${seq}`,
    seq_id: seq,
    title: `task ${seq}`,
    status_id: 'todo',
    progress_pct: 0,
    planned_start_on: null,
    soft_deadline: null,
    hard_deadline: null,
    ...overrides,
  };
}

const self = task(42, {
  planned_start_on: '2026-11-16',
  soft_deadline: '2026-11-20T00:00:00.000Z',
  hard_deadline: '2026-11-24T00:00:00.000Z',
});

function build(blockedBy: BlockingChartTask[], blocks: BlockingChartTask[], me = self) {
  return buildBlockingChart({ task: me, blockedBy, blocks, statuses, projectKey: 'ENG', today });
}

describe('buildBlockingChart', () => {
  it('行は 待たせている相手 → このタスク → 待っている相手 の順', () => {
    const chart = build([task(30), task(31)], [task(50)]);
    expect(chart.labels.map((l) => [l.seqKey, l.role])).toEqual([
      ['ENG-30', 'blocked_by'],
      ['ENG-31', 'blocked_by'],
      ['ENG-42', 'self'],
      ['ENG-50', 'blocks'],
    ]);
    expect(chart.tasks.map((t) => t.id)).toEqual(['t30', 't31', 't42', 't50']);
    expect(chart.rows.map((r) => (r.kind === 'task' ? r.task_id : null))).toEqual([
      't30',
      't31',
      't42',
      't50',
    ]);
  });

  it('棒は 開始〜締切。締切が無ければ目標まで', () => {
    const chart = build([], [], self);
    expect(chart.tasks[0]).toMatchObject({ start: '2026-11-16', end: '2026-11-24' });
    const soft = build(
      [],
      [],
      task(1, { planned_start_on: '2026-11-10', soft_deadline: '2026-11-12' }),
    );
    expect(soft.tasks[0]).toMatchObject({ start: '2026-11-10', end: '2026-11-12' });
  });

  it('依存は 済み = done、相手の期限が開始より後 = delayed、それ以外 = normal', () => {
    const finished = task(30, { status_id: 'done', hard_deadline: '2026-11-30' });
    const late = task(31, { planned_start_on: '2026-11-10', hard_deadline: '2026-11-21' });
    const onTime = task(32, { hard_deadline: '2026-11-16' });
    const chart = build([finished, late, onTime], []);
    expect(chart.deps).toEqual([
      { blocker_task_id: 't30', blocked_task_id: 't42', state: 'done' },
      { blocker_task_id: 't31', blocked_task_id: 't42', state: 'delayed' },
      // 期限と開始が同じ日は遅れではない
      { blocker_task_id: 't32', blocked_task_id: 't42', state: 'normal' },
    ]);
    expect(chart.rows[0]).toMatchObject({ state: 'done' });
    expect(chart.rows[1]).toMatchObject({ state: 'normal' });
  });

  it('注記は delayed ごとに 1 行。このタスクは「このタスク」と書く', () => {
    const late = task(30, { hard_deadline: '2026-11-21' });
    const waiting = task(50, { planned_start_on: '2026-11-18' });
    const chart = build([late], [waiting]);
    expect(chart.notes).toEqual([
      '点線：ENG-30 の期限（11/21）が、このタスクの開始（11/16）より後',
      '点線：このタスクの期限（11/24）が、ENG-50 の開始（11/18）より後',
    ]);
    expect(build([task(30, { hard_deadline: '2026-11-10' })], []).notes).toEqual([]);
  });

  it('日付が足りない依存は normal で、注記も出さない', () => {
    // ブロックする側に期限が無い
    const noDeadline = task(30, { planned_start_on: '2026-12-01' });
    // される側に日付が無い
    const undated = task(50);
    const chart = build([noDeadline], [undated]);
    expect(chart.deps.map((d) => d.state)).toEqual(['normal', 'normal']);
    expect(chart.notes).toEqual([]);
  });

  it('開始だけは 1 日の棒、期限だけは期限の日の点、どちらも無ければ今日の 1 日の棒', () => {
    const startOnly = task(30, { planned_start_on: '2026-11-12' });
    const deadlineOnly = task(31, { soft_deadline: '2026-11-19T00:00:00.000Z' });
    const none = task(32);
    const chart = build([startOnly, deadlineOnly, none], []);
    expect(chart.tasks[0]).toMatchObject({ start: '2026-11-12', end: '2026-11-12' });
    expect(chart.rows[0]).not.toHaveProperty('shape');
    expect(chart.tasks[1]).toMatchObject({ start: '2026-11-19', end: '2026-11-19' });
    expect(chart.rows[1]).toMatchObject({ shape: 'point' });
    expect(chart.tasks[2]).toMatchObject({ start: today, end: today });
    expect(chart.rows[2]).not.toHaveProperty('shape');
    expect(chart.labels.map((l) => l.undated)).toEqual([false, false, true, false]);
  });

  it('このタスクに日付が無くても図の行になる', () => {
    const chart = build([task(30, { hard_deadline: '2026-11-20' })], [], task(42));
    expect(chart.tasks.map((t) => t.id)).toEqual(['t30', 't42']);
    expect(chart.tasks[1]).toMatchObject({ start: today, end: today });
    expect(chart.deps[0].state).toBe('normal');
  });

  it('期間は最初の日〜最後の日に今日を含め、前後 2 日を空ける', () => {
    const chart = build(
      [task(30, { planned_start_on: '2026-11-05', hard_deadline: '2026-11-08' })],
      [],
    );
    expect(chart.view).toEqual({ from: '2026-11-03', to: '2026-11-26' });
    // 全部の棒が今日より後でも、今日を含める
    const future = build(
      [],
      [],
      task(42, { planned_start_on: '2026-11-25', hard_deadline: '2026-11-28' }),
    );
    expect(future.view).toEqual({ from: '2026-11-14', to: '2026-11-30' });
  });

  it('60 日を超えたら今日を中心の 60 日に切る', () => {
    const days = (view: { from: string; to: string }) =>
      (Date.parse(view.to) - Date.parse(view.from)) / 86_400_000 + 1;
    // 前後 2 日を足してちょうど 60 日: 切らない
    const exact = build(
      [],
      [],
      task(42, { planned_start_on: '2026-11-16', hard_deadline: '2027-01-10' }),
    );
    expect(days(exact.view)).toBe(60);
    expect(exact.view).toEqual({ from: '2026-11-14', to: '2027-01-12' });
    // 61 日: 切る
    const over = build(
      [],
      [],
      task(42, { planned_start_on: '2026-11-16', hard_deadline: '2027-01-11' }),
    );
    expect(over.view).toEqual({ from: '2026-10-17', to: '2026-12-15' });
    expect(days(over.view)).toBe(60);
  });
});

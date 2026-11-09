import { describe, expect, it } from 'vitest';

import { UNASSIGNED_MEMBER } from '@/lib/calendar/filters';

import {
  layoutGantt,
  monthDay,
  taskDates,
  taskIdOfRow,
  warnNote,
  type GanttSource,
  type LayoutOptions,
} from '../layout';

const TZ = 'Asia/Tokyo';

function task(over: Partial<GanttSource> & { id: string }): GanttSource {
  return {
    seq_key: `REL-${over.id}`,
    title: `課題 ${over.id}`,
    progress_pct: 0,
    assignees: [],
    project: { id: 'p-rel', key: 'REL', name: 'Release' },
    status: { is_done_state: false },
    ...over,
  };
}

/** 東京の 9 時（暦の日がずれない時刻） */
function at(date: string): string {
  return `${date}T00:00:00Z`;
}

function options(over: Partial<LayoutOptions> = {}): LayoutOptions {
  return {
    groupBy: 'project',
    collapsed: new Set(),
    memberIds: [],
    members: [
      { id: 'u-sato', username: '佐藤' },
      { id: 'u-suzuki', username: '鈴木' },
    ],
    timeZone: TZ,
    ...over,
  };
}

describe('taskDates', () => {
  it('予定開始〜目安の期限を棒にし、厳守の期限は棒に含めない', () => {
    expect(
      taskDates(
        task({
          id: '1',
          planned_start_on: '2026-11-02',
          soft_deadline: at('2026-11-10'),
          hard_deadline: at('2026-11-12'),
        }),
        TZ,
      ),
    ).toEqual({
      start: '2026-11-02',
      last: '2026-11-10',
      point: false,
      soft: '2026-11-10',
      hard: '2026-11-12',
      warn: false,
    });
  });

  it('予定開始の無い課題は、目安の期限の日の点にする', () => {
    const dates = taskDates(task({ id: '1', soft_deadline: at('2026-11-12') }), TZ)!;
    expect(dates).toMatchObject({ start: '2026-11-12', last: '2026-11-12', point: true });
  });

  it('目安が無ければ厳守の期限で終える', () => {
    const dates = taskDates(
      task({ id: '1', planned_start_on: '2026-11-02', hard_deadline: at('2026-11-06') }),
      TZ,
    )!;
    expect(dates).toMatchObject({ last: '2026-11-06', hard: '2026-11-06', warn: false });
  });

  it('目安が厳守を越えた課題に印を付け、棒は目安で終える', () => {
    const dates = taskDates(
      task({
        id: '1',
        planned_start_on: '2026-11-09',
        soft_deadline: at('2026-11-14'),
        hard_deadline: at('2026-11-12'),
      }),
      TZ,
    )!;
    expect(dates).toMatchObject({ last: '2026-11-14', warn: true });
    expect(warnNote(dates)).toBe('目安 11/14 が厳守 11/12 を越えた');
  });

  it('予定開始が期限より後でも、逆向きの棒にしない', () => {
    const dates = taskDates(
      task({ id: '1', planned_start_on: '2026-11-12', soft_deadline: at('2026-11-10') }),
      TZ,
    )!;
    expect(dates).toMatchObject({ start: '2026-11-12', last: '2026-11-12' });
  });

  it('締切は暦の日に落とす（東京で日付が変わる時刻）', () => {
    // UTC の 11/9 16:00 は東京の 11/10 1:00
    const dates = taskDates(task({ id: '1', soft_deadline: '2026-11-09T16:00:00Z' }), TZ)!;
    expect(dates.last).toBe('2026-11-10');
  });

  it('日付が一つも無い課題は null', () => {
    expect(taskDates(task({ id: '1' }), TZ)).toBeNull();
  });
});

describe('layoutGantt', () => {
  const rel1 = task({
    id: '1',
    planned_start_on: '2026-11-04',
    soft_deadline: at('2026-11-10'),
    assignees: [{ user_id: 'u-sato' }],
  });
  const rel2 = task({
    id: '2',
    planned_start_on: '2026-11-02',
    soft_deadline: at('2026-11-05'),
    assignees: [{ user_id: 'u-suzuki' }, { user_id: 'u-sato' }],
    status: { is_done_state: true },
  });
  const mob1 = task({
    id: '3',
    seq_key: 'MOB-1',
    project: { id: 'p-mob', key: 'MOB', name: 'Mobile' },
    soft_deadline: at('2026-11-14'),
    hard_deadline: at('2026-11-12'),
  });
  const undated = task({ id: '4' });

  it('プロジェクトで群れ、群れの中は棒の始まりの順にする', () => {
    const layout = layoutGantt([rel1, mob1, rel2, undated], options());
    expect(layout.groups.map((g) => [g.label, g.count])).toEqual([
      ['Release', 2],
      ['Mobile', 1],
    ]);
    expect(layout.groups[0]!.items.map((i) => i.task.id)).toEqual(['2', '1']);
    expect(layout.rows.map((r) => (r.kind === 'group' ? 'group' : r.task_id))).toEqual([
      'group',
      '2',
      '1',
      'group',
      '3',
    ]);
    expect(layout.skipped).toBe(1);
  });

  it('arc の棒の終わりは最後の日の翌日（arc の end は境の日）', () => {
    const layout = layoutGantt([rel1], options());
    expect(layout.tasks).toEqual([
      { id: '1', title: '課題 1', progress_pct: 0, start: '2026-11-04', end: '2026-11-11' },
    ]);
  });

  it('行の印: 完了は done、目安が厳守を越えたら warn と添えの字、予定開始が無ければ点', () => {
    const layout = layoutGantt([rel1, rel2, mob1], options());
    const rowOf = (id: string) => layout.rows.find((r) => r.kind === 'task' && r.task_id === id);
    expect(rowOf('2')).toMatchObject({ state: 'done', shape: 'bar', note: null });
    expect(rowOf('1')).toMatchObject({ state: null, shape: 'bar', hard_deadline: null });
    expect(rowOf('3')).toMatchObject({
      state: 'warn',
      shape: 'point',
      hard_deadline: '2026-11-12',
      note: '目安 11/14 が厳守 11/12 を越えた',
    });
  });

  it('畳んだ群れは見出しの行と数だけを残し、課題を arc に渡さない', () => {
    const layout = layoutGantt([rel1, rel2, mob1], options({ collapsed: new Set(['p-rel']) }));
    expect(layout.groups[0]).toMatchObject({ key: 'p-rel', count: 2, collapsed: true });
    expect(layout.rows.map((r) => (r.kind === 'group' ? 'group' : r.task_id))).toEqual([
      'group',
      'group',
      '3',
    ]);
    expect(layout.tasks.map((t) => t.id)).toEqual(['3']);
  });

  it('担当者で群れると、二人で持つ課題は二人の群れの両方に出て、担当なしは最後', () => {
    const layout = layoutGantt([rel1, rel2, mob1], options({ groupBy: 'assignee' }));
    expect(layout.groups.map((g) => [g.label, g.items.map((i) => i.rowId)])).toEqual([
      ['佐藤', ['2:u-sato', '1:u-sato']],
      ['鈴木', ['2:u-suzuki']],
      ['担当なし', ['3']],
    ]);
    // arc に同じ id を二度渡さない
    const ids = layout.tasks.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(taskIdOfRow('2:u-sato')).toBe('2');
    expect(taskIdOfRow('3')).toBe('3');
  });

  it('担当者で絞ると、選んだ者の課題だけ、選んだ者の群れにだけ出す', () => {
    const byProject = layoutGantt([rel1, rel2, mob1], options({ memberIds: ['u-suzuki'] }));
    expect(byProject.tasks.map((t) => t.id)).toEqual(['2']);
    const byAssignee = layoutGantt(
      [rel1, rel2, mob1],
      options({ groupBy: 'assignee', memberIds: ['u-suzuki'] }),
    );
    expect(byAssignee.groups.map((g) => g.label)).toEqual(['鈴木']);
    const unassigned = layoutGantt([rel1, mob1], options({ memberIds: [UNASSIGNED_MEMBER] }));
    expect(unassigned.tasks.map((t) => t.id)).toEqual(['3']);
  });

  it('月/日の字', () => {
    expect(monthDay('2026-01-05')).toBe('1/5');
  });
});

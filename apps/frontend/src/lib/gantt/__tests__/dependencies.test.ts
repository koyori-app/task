import { describe, expect, it } from 'vitest';

import { dependencyEdges, dependencyState, type ScheduleRelation } from '../dependencies';
import { layoutGantt, type GanttSource, type LayoutOptions } from '../layout';

const TZ = 'Asia/Tokyo';
const TODAY = '2026-11-11';
const REL = { id: 'p-rel', key: 'REL', name: 'Release' };
const MOB = { id: 'p-mob', key: 'MOB', name: 'Mobile' };

function task(over: Partial<GanttSource> & { id: string; seq_key: string }): GanttSource {
  return {
    title: over.id,
    progress_pct: 0,
    assignees: [],
    project: REL,
    status: { is_done_state: false },
    ...over,
  };
}

const TASKS: GanttSource[] = [
  // 阻む課題。完了しておらぬ
  task({
    id: 'a',
    seq_key: 'REL-1',
    planned_start_on: '2026-11-02',
    soft_deadline: '2026-11-06T00:00:00Z',
    assignees: [{ user_id: 'u1' }],
  }),
  // 予定開始が今日（11/11）より前: a を待ちで遅れ
  task({
    id: 'b',
    seq_key: 'REL-2',
    planned_start_on: '2026-11-09',
    soft_deadline: '2026-11-13T00:00:00Z',
    assignees: [{ user_id: 'u1' }, { user_id: 'u2' }],
  }),
  // 予定開始が今日より後: 普通
  task({
    id: 'c',
    seq_key: 'REL-3',
    planned_start_on: '2026-11-16',
    soft_deadline: '2026-11-20T00:00:00Z',
    assignees: [{ user_id: 'u2' }],
  }),
  // 完了した阻む課題
  task({
    id: 'd',
    seq_key: 'REL-4',
    planned_start_on: '2026-11-02',
    soft_deadline: '2026-11-04T00:00:00Z',
    status: { is_done_state: true },
  }),
  // 別のプロジェクト
  task({
    id: 'm',
    seq_key: 'MOB-1',
    project: MOB,
    planned_start_on: '2026-11-10',
    soft_deadline: '2026-11-12T00:00:00Z',
    assignees: [{ user_id: 'u2' }],
  }),
];

function layout(over: Partial<LayoutOptions> = {}) {
  return layoutGantt(TASKS, {
    groupBy: 'project',
    collapsed: new Set(),
    memberIds: [],
    members: [
      { id: 'u1', username: 'alice' },
      { id: 'u2', username: 'bob' },
    ],
    timeZone: TZ,
    today: TODAY,
    ...over,
  });
}

function edges(relations: ScheduleRelation[], over: Partial<LayoutOptions> = {}) {
  return dependencyEdges({
    relations,
    tasks: TASKS,
    layout: layout(over),
    timeZone: TZ,
    today: TODAY,
  });
}

const rel = (from: string, to: string): ScheduleRelation => ({
  from_task_id: from,
  to_task_id: to,
});
const byId = new Map(TASKS.map((t) => [t.id, t]));

describe('dependencyState', () => {
  it('済んだ阻みは done、予定開始が今日以前で阻みが済まぬなら delayed、まだ来ぬなら normal', () => {
    expect(dependencyState(byId.get('d')!, byId.get('b')!, TZ, TODAY)).toBe('done');
    expect(dependencyState(byId.get('a')!, byId.get('b')!, TZ, TODAY)).toBe('delayed');
    expect(dependencyState(byId.get('a')!, byId.get('c')!, TZ, TODAY)).toBe('normal');
  });

  it('阻まれる課題が自ら済んでおれば、予定開始を過ぎていても待ちで遅れにしない', () => {
    const doneB = { ...byId.get('b')!, status: { is_done_state: true } };
    expect(dependencyState(byId.get('a')!, doneB, TZ, TODAY)).toBe('normal');
    // 阻む課題が済んでおれば、阻まれる課題が済んでいても done のまま
    expect(dependencyState(byId.get('d')!, doneB, TZ, TODAY)).toBe('done');
  });

  it('予定開始の当日は遅れ（今日以前）', () => {
    const today = '2026-11-09';
    expect(dependencyState(byId.get('a')!, byId.get('b')!, TZ, today)).toBe('delayed');
    expect(dependencyState(byId.get('a')!, byId.get('b')!, TZ, '2026-11-08')).toBe('normal');
  });
});

describe('dependencyEdges', () => {
  it('両端が図にある辺は行を結び、待ちで遅れは阻まれる棒に「<キー> 待ち」を添える', () => {
    const { edges: out } = edges([rel('a', 'b'), rel('a', 'c'), rel('d', 'm')]);
    expect(out).toEqual([
      { blocker_task_id: 'a', blocked_task_id: 'b', state: 'delayed', note: 'REL-1 待ち' },
      { blocker_task_id: 'a', blocked_task_id: 'c', state: 'normal' },
      { blocker_task_id: 'd', blocked_task_id: 'm', state: 'done' },
    ]);
  });

  it('担当者で群れて同じ課題が幾つも出る時は、最初の行を結ぶ', () => {
    const { edges: out } = edges([rel('a', 'b')], { groupBy: 'assignee' });
    // b は alice と bob の両方の群れに出る。名の順で alice が先
    expect(out[0]).toMatchObject({ blocker_task_id: 'a:u1', blocked_task_id: 'b:u1' });
  });

  it('畳んだ群れの中の端は、その群れの見出しの行の始まりの日へ引き、キーを添える', () => {
    const { edges: out } = edges([rel('a', 'm')], { collapsed: new Set(['p-mob']) });
    // 行: 0 Release の見出し、1〜4 Release の課題、5 Mobile の見出し（畳んである）
    expect(out).toEqual([
      {
        blocker_task_id: 'a',
        blocked_task_id: 'm',
        state: 'delayed',
        blocked_at: { row: 5, day: '2026-11-10', label: 'MOB-1' },
      },
    ]);
  });

  it('絞りで図に出ぬ端は、もう一方の行の端まで引き、キーと始まりの日を添える', () => {
    // bob で絞ると a（alice だけ）は図に出ぬ。a が阻む c は bob の課題。c が阻む a は予定開始（11/2）を過ぎて遅れ
    // （阻まれる側が図に出ぬので、待ちの添えの字は付けぬ）
    const { edges: out } = edges([rel('a', 'c'), rel('c', 'a')], { memberIds: ['u2'] });
    expect(out).toEqual([
      {
        blocker_task_id: 'a',
        blocked_task_id: 'c',
        state: 'normal',
        blocker_at: { edge: 'start', label: 'REL-1 11/2' },
      },
      {
        blocker_task_id: 'c',
        blocked_task_id: 'a',
        state: 'delayed',
        blocked_at: { edge: 'end', label: 'REL-1 11/2' },
      },
    ]);
  });

  it('両端が同じ畳んだ群れの中の辺は、同じ見出しの行に潰れるので描かぬ', () => {
    // a と b はどちらも Release。Release を畳むと両端が同じ見出しの行（0）へ来る
    const { edges: out, marks } = edges([rel('a', 'b')], { collapsed: new Set(['p-rel']) });
    expect(out).toEqual([]);
    // 狭い画面の印は、行が見えておらずとも数えておく
    expect(marks.get('b')).toEqual({ count: 1, waitingFor: ['REL-1'] });
  });

  it('畳んだ群れの見出しの行の番号は、前の群れの見出しと行を数えた arc の rows の添字', () => {
    const l = layout({ collapsed: new Set(['p-mob']) });
    const { edges: out } = dependencyEdges({
      relations: [rel('a', 'm')],
      tasks: TASKS,
      layout: l,
      timeZone: TZ,
      today: TODAY,
    });
    const row = (out[0]!.blocked_at as { row: number }).row;
    expect(l.rows[row]).toEqual({ kind: 'group' });
    // Release の見出しと課題四つの後
    expect(row).toBe(5);
  });

  it('畳んだ群れが前に続いても、見出しの行の添字は畳んだ群れの課題を数えぬ', () => {
    // 担当者で群れ、alice と bob を畳む。行: 0 alice（畳み）、1 bob（畳み）、2 担当なし、3 REL-4
    const l = layout({ groupBy: 'assignee', collapsed: new Set(['u1', 'u2']) });
    const { edges: out } = dependencyEdges({
      relations: [rel('d', 'c')],
      tasks: TASKS,
      layout: l,
      timeZone: TZ,
      today: TODAY,
    });
    expect(out).toEqual([
      {
        blocker_task_id: 'd',
        blocked_task_id: 'c',
        state: 'done',
        blocked_at: { row: 1, day: '2026-11-16', label: 'REL-3' },
      },
    ]);
    expect(l.rows[1]).toEqual({ kind: 'group' });
  });

  it('両端とも図の行に無い辺と、予定の課題に無い端の辺は描かぬ', () => {
    const { edges: out } = edges([rel('a', 'd'), rel('a', 'zz')], { memberIds: ['u2'] });
    expect(out).toEqual([]);
  });

  it('狭い画面の印: 阻む側も阻まれる側も数え、待ちで遅れは阻む課題のキーを並べる', () => {
    const { marks } = edges([rel('a', 'b'), rel('d', 'b'), rel('a', 'c')]);
    expect(marks.get('a')).toEqual({ count: 2, waitingFor: [] });
    expect(marks.get('b')).toEqual({ count: 2, waitingFor: ['REL-1'] });
    expect(marks.get('c')).toEqual({ count: 1, waitingFor: [] });
    expect(marks.get('m')).toBeUndefined();
  });
});

describe('済んだ棒の「待ち」', () => {
  it('阻まれる課題が済んでおれば、辺の添えにも行の印にも「<キー> 待ち」を付けない', () => {
    const tasks = [byId.get('a')!, { ...byId.get('b')!, status: { is_done_state: true } }];
    const { edges: out, marks } = dependencyEdges({
      relations: [rel('a', 'b')],
      tasks,
      layout: layoutGantt(tasks, {
        groupBy: 'project',
        collapsed: new Set(),
        memberIds: [],
        members: [],
        timeZone: TZ,
        today: TODAY,
      }),
      timeZone: TZ,
      today: TODAY,
    });
    expect(out).toEqual([{ blocker_task_id: 'a', blocked_task_id: 'b', state: 'normal' }]);
    expect(marks.get('b')?.waitingFor).toEqual([]);
  });
});

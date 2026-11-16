import { monthDay, taskDates, type GanttLayout, type GanttSource } from '@/lib/gantt/layout';

/** 予定の関係の口の一件。向きは常に blocks（`from` が阻み、`to` が阻まれる） */
export type ScheduleRelation = { from_task_id: string; to_task_id: string };

/** 辺の状態。arc の `GanttDep.state` と同じ値 */
export type DependencyState = 'normal' | 'done' | 'delayed';

/** 辺の端の代わり。arc の `GanttDep.blocker_at`・`blocked_at` と同じ形 */
export type DependencyEnd =
  | { row: number; day: string; label: string }
  | { edge: 'start' | 'end'; label: string };

/** arc へ渡す辺。arc の `GanttDep` の鍵のある形と同じ */
export type DependencyEdge = {
  blocker_task_id: string;
  blocked_task_id: string;
  state: DependencyState;
  note?: string;
  blocker_at?: DependencyEnd;
  blocked_at?: DependencyEnd;
};

/** 狭い画面の行の添えに出す、その課題の依存の印 */
export type DependencyMark = {
  /** その課題が阻む・阻まれる関係の数 */
  count: number;
  /** 完了しておらぬのに、この課題の予定開始を過ぎても阻んでおる課題のキー */
  waitingFor: string[];
};

export type DependencyInput = {
  relations: readonly ScheduleRelation[];
  /** 予定の口が返した課題。絞りや畳みで図に出ぬ課題も含む */
  tasks: readonly GanttSource[];
  layout: GanttLayout;
  timeZone: string;
  /** 今日（タイムゾーンの暦の日） */
  today: string;
};

/**
 * 辺の状態を決める。阻む課題が完了なら `done`。完了しておらず、阻まれる課題の予定開始
 * （無ければ図の棒の始まり）が今日以前なら `delayed`。それ以外は `normal`。
 */
export function dependencyState(
  blocker: GanttSource,
  blocked: GanttSource,
  timeZone: string,
  today: string,
): DependencyState {
  if (blocker.status.is_done_state) return 'done';
  const start = taskDates(blocked, timeZone)?.start;
  return start && start <= today ? 'delayed' : 'normal';
}

/**
 * 予定の関係を、arc へ渡す辺と、狭い画面の行の印に組む（cmd_426 の六つの決め）。
 *
 * - 両端が図の行にある辺は、その行（担当者で群れて一つの課題が幾つも出る時は最初の行）を結ぶ。
 * - 畳んだ群れの中の端は、その群れの見出しの行の、課題の始まりの日へ引き、キーを添える。
 * - 絞りで図に出ぬ端は、もう一方の端の行の端まで引き、キーと始まりの日を添える。
 *   阻まれる課題が外なら右の端、阻む課題が外なら左の端。
 * - 待ちで遅れの辺は、阻まれる棒の右に「<阻む課題のキー> 待ち」を添える。
 */
export function dependencyEdges(input: DependencyInput): {
  edges: DependencyEdge[];
  marks: Map<string, DependencyMark>;
} {
  const byId = new Map(input.tasks.map((task) => [task.id, task]));
  // 課題の id から、図に出る最初の行と、畳んだ群れの見出しの行の番号へ
  const rowOf = new Map<string, string>();
  const collapsedRowOf = new Map<string, number>();
  let rowIndex = 0;
  for (const group of input.layout.groups) {
    const groupRow = rowIndex;
    rowIndex += 1;
    for (const item of group.items) {
      if (group.collapsed) {
        if (!collapsedRowOf.has(item.task.id)) collapsedRowOf.set(item.task.id, groupRow);
        continue;
      }
      if (!rowOf.has(item.task.id)) rowOf.set(item.task.id, item.rowId);
    }
    if (!group.collapsed) rowIndex += group.items.length;
  }

  const edges: DependencyEdge[] = [];
  const marks = new Map<string, DependencyMark>();
  const markOf = (id: string) => {
    let mark = marks.get(id);
    if (!mark) {
      mark = { count: 0, waitingFor: [] };
      marks.set(id, mark);
    }
    return mark;
  };

  for (const relation of input.relations) {
    const blocker = byId.get(relation.from_task_id);
    const blocked = byId.get(relation.to_task_id);
    if (!blocker || !blocked || blocker.id === blocked.id) continue;
    const state = dependencyState(blocker, blocked, input.timeZone, input.today);
    markOf(blocker.id).count += 1;
    markOf(blocked.id).count += 1;
    if (state === 'delayed') markOf(blocked.id).waitingFor.push(blocker.seq_key);

    const end = (
      task: GanttSource,
      side: 'start' | 'end',
    ): { id: string; at?: DependencyEnd } | null => {
      const row = rowOf.get(task.id);
      if (row) return { id: row };
      const start = taskDates(task, input.timeZone)?.start;
      if (!start) return null;
      const collapsedRow = collapsedRowOf.get(task.id);
      if (collapsedRow !== undefined) {
        return { id: task.id, at: { row: collapsedRow, day: start, label: task.seq_key } };
      }
      return { id: task.id, at: { edge: side, label: `${task.seq_key} ${monthDay(start)}` } };
    };
    const from = end(blocker, 'start');
    const to = end(blocked, 'end');
    // 両端とも図の行に無い辺は、引く元の行が無い
    if (!from || !to || (from.at && to.at)) continue;
    edges.push({
      blocker_task_id: from.id,
      blocked_task_id: to.id,
      state,
      ...(state === 'delayed' && !to.at ? { note: `${blocker.seq_key} 待ち` } : {}),
      ...(from.at ? { blocker_at: from.at } : {}),
      ...(to.at ? { blocked_at: to.at } : {}),
    });
  }
  return { edges, marks };
}

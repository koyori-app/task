import type { GanttDep, GanttRow, GanttTask, ViewRange } from '@koyori-app/arc-vue';

import { formatMonthDay } from '@/lib/task-date-display';
import { isoToLocalDateInput, taskSeqKey } from '@/lib/task-display';

/** 図に要るタスクの項目だけ（TaskResponse / TaskDetailResponse / RelationEntry のどれでも渡せる） */
export interface BlockingChartTask {
  id: string;
  seq_id: number;
  title: string;
  status_id: string;
  progress_pct: number;
  planned_start_on?: string | null;
  soft_deadline?: string | null;
  hard_deadline?: string | null;
}

interface ChartStatus {
  id: string;
  is_done_state: boolean;
}

/** 図の 1 行。左のラベルの列と右の棒が同じ並びになる */
interface BlockingChartLabel<T extends BlockingChartTask = BlockingChartTask> {
  task: T;
  seqKey: string;
  /** blocked_by = このタスクを待たせている相手、self = このタスク、blocks = このタスクを待っている相手 */
  role: 'blocked_by' | 'self' | 'blocks';
  done: boolean;
  /** 開始も期限も無く、今日の 1 日の棒で仮に出している */
  undated: boolean;
}

export interface BlockingChart<T extends BlockingChartTask = BlockingChartTask> {
  labels: BlockingChartLabel<T>[];
  tasks: GanttTask[];
  rows: GanttRow[];
  deps: GanttDep[];
  view: ViewRange;
  /** 食い違い（依存の点線）ごとの注記 */
  notes: string[];
}

const DAY_MS = 86_400_000;
/** 図に出す最長の日数。これを超えたら今日を中心に切る */
const MAX_VIEW_DAYS = 60;
/** 最初の日の前と最後の日の後に空ける日数 */
const VIEW_PADDING_DAYS = 2;

function dayTime(day: string): number {
  const [year, month, date] = day.split('-').map(Number);
  return Date.UTC(year, month - 1, date);
}

function addDays(day: string, days: number): string {
  return new Date(dayTime(day) + days * DAY_MS).toISOString().slice(0, 10);
}

function day(value: string | null | undefined): string | null {
  return isoToLocalDateInput(value) || null;
}

/** 棒の日付。開始だけ = その日の 1 日の棒、期限だけ = 期限の日の点、どちらも無い = 今日の 1 日の棒 */
function span(task: BlockingChartTask, today: string) {
  const planned = day(task.planned_start_on);
  const deadline = day(task.hard_deadline) ?? day(task.soft_deadline);
  const start = planned ?? day(task.soft_deadline) ?? day(task.hard_deadline);
  if (!start) return { start: today, end: today, deadline: null, begins: null, undated: true };
  return {
    start,
    end: deadline ?? start,
    deadline,
    // 依存の食い違いを見るときの「開始」。開始予定日が無ければ比べない（期限の日を開始と呼ばない）
    begins: planned,
    undated: false,
    point: !planned,
  };
}

/**
 * ブロック関係の小さなガント。行は「待たせている相手 → このタスク → 待っている相手」の順。
 *
 * 依存の線は、ブロックする側が完了していれば done、ブロックする側の期限がされる側の開始より
 * 後なら delayed（点線）、それ以外と日付が足りないときは normal。
 */
export function buildBlockingChart<T extends BlockingChartTask>(input: {
  task: BlockingChartTask;
  blockedBy: readonly T[];
  blocks: readonly T[];
  statuses: readonly ChartStatus[];
  projectKey: string;
  /** 見る人の今日（`YYYY-MM-DD`） */
  today: string;
}): BlockingChart<T | BlockingChartTask> {
  const { task: self, today, projectKey } = input;
  const done = new Set(input.statuses.filter((s) => s.is_done_state).map((s) => s.id));
  const entries: { task: T | BlockingChartTask; role: BlockingChartLabel['role'] }[] = [
    ...input.blockedBy.map((task) => ({ task, role: 'blocked_by' as const })),
    { task: self, role: 'self' as const },
    ...input.blocks.map((task) => ({ task, role: 'blocks' as const })),
  ];

  const spans = new Map(entries.map(({ task }) => [task.id, span(task, today)]));
  const name = (task: BlockingChartTask) =>
    task.id === self.id ? 'このタスク' : `${taskSeqKey(projectKey, task.seq_id)} `;

  const labels = entries.map(({ task, role }) => ({
    task,
    role,
    seqKey: taskSeqKey(projectKey, task.seq_id),
    done: done.has(task.status_id),
    undated: spans.get(task.id)!.undated,
  }));
  const tasks: GanttTask[] = entries.map(({ task }) => {
    const { start, end } = spans.get(task.id)!;
    return { id: task.id, title: task.title, progress_pct: task.progress_pct, start, end };
  });
  const rows: GanttRow[] = entries.map(({ task }) => ({
    kind: 'task',
    task_id: task.id,
    // 棒の色は完了かどうかだけ。ステータスごとの色は arc の対応待ち（koyori-app/arc#83）
    state: done.has(task.status_id) ? 'done' : 'normal',
    ...(spans.get(task.id)!.point ? { shape: 'point' as const } : {}),
  }));

  const notes: string[] = [];
  const pairs = [
    ...input.blockedBy.map((blocker) => ({ blocker, blocked: self })),
    ...input.blocks.map((blocked) => ({ blocker: self, blocked })),
  ];
  const deps: GanttDep[] = pairs.map(({ blocker, blocked }) => {
    const base = { blocker_task_id: blocker.id, blocked_task_id: blocked.id };
    if (done.has(blocker.status_id)) return { ...base, state: 'done' };
    const deadline = spans.get(blocker.id)!.deadline;
    const begins = spans.get(blocked.id)!.begins;
    if (deadline && begins && deadline > begins) {
      notes.push(
        `点線：${name(blocker)}の期限（${formatMonthDay(deadline)}）が、${name(blocked)}の開始（${formatMonthDay(begins)}）より後`,
      );
      return { ...base, state: 'delayed' };
    }
    return { ...base, state: 'normal' };
  });

  const days = [today, ...tasks.flatMap((task) => [task.start, task.end ?? task.start])].sort();
  let view: ViewRange = {
    from: addDays(days[0], -VIEW_PADDING_DAYS),
    to: addDays(days[days.length - 1], VIEW_PADDING_DAYS),
  };
  if ((dayTime(view.to) - dayTime(view.from)) / DAY_MS + 1 > MAX_VIEW_DAYS) {
    view = {
      from: addDays(today, -MAX_VIEW_DAYS / 2),
      to: addDays(today, MAX_VIEW_DAYS / 2 - 1),
    };
  }

  return { labels, tasks, rows, deps, view, notes };
}

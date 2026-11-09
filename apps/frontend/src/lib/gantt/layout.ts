import type { GanttRow, GanttTask } from '@koyori-app/arc-vue';

import { matchesMembers, UNASSIGNED_MEMBER } from '@/lib/calendar/filters';
import { addDays, dateInZone } from '@/lib/calendar/timeline';
import type { GanttGroupBy } from '@/lib/gantt/view';

/** ガントに描く課題。予定の API の `ScheduleTask` から使う欄だけを取る */
export type GanttSource = {
  id: string;
  seq_key: string;
  title: string;
  progress_pct: number;
  planned_start_on?: string | null;
  soft_deadline?: string | null;
  hard_deadline?: string | null;
  assignees: readonly { user_id: string }[];
  project: { id: string; key: string; name: string };
  status: { is_done_state: boolean };
};

export type GanttMember = { id: string; username: string };

/** 一つの課題の日付の読み。日付はどれもタイムゾーンの暦の日（`YYYY-MM-DD`） */
export type TaskDates = {
  /** 棒の始まり。予定開始が無い課題は点の日 */
  start: string;
  /** 棒の最後の日（含む）。目安の期限、無ければ厳守の期限、無ければ予定開始 */
  last: string;
  /** 予定開始が無く、棒でなく点で描く課題 */
  point: boolean;
  soft: string | null;
  hard: string | null;
  /** 目安の期限が厳守の期限を越えた */
  warn: boolean;
};

/**
 * 課題の日付を棒の形に読む。予定開始〜目安の期限を棒にし、厳守の期限は棒に含めず菱形で別に描く。
 * 目安と厳守が逆に入っていても（サーバーは検証しない）、棒は目安で終え、越えた印を付ける。
 * 日付が一つも無い課題は `null`（期間なしの数に入る）。
 */
export function taskDates(task: GanttSource, timeZone: string): TaskDates | null {
  const soft = task.soft_deadline ? dateInZone(task.soft_deadline, timeZone) : null;
  const hard = task.hard_deadline ? dateInZone(task.hard_deadline, timeZone) : null;
  const start = task.planned_start_on ?? null;
  const end = soft ?? hard ?? start;
  if (!end) return null;
  const warn = !!soft && !!hard && soft > hard;
  if (!start) return { start: end, last: end, point: true, soft, hard, warn };
  // 予定開始が期限より後に入っていても、逆向きの棒にしない
  return { start, last: end < start ? start : end, point: false, soft, hard, warn };
}

/** 月/日（例「11/14」） */
export function monthDay(date: string): string {
  const [, month, day] = date.split('-');
  return `${Number(month)}/${Number(day)}`;
}

/** 目安が厳守を越えた行に添える字 */
export function warnNote(dates: TaskDates): string | null {
  if (!dates.warn || !dates.soft || !dates.hard) return null;
  return `目安 ${monthDay(dates.soft)} が厳守 ${monthDay(dates.hard)} を越えた`;
}

/** 左の一覧の一行 */
export type GanttListItem = {
  /** arc に渡す課題の id。担当者で群れると同じ課題が二つの群れに出るので、課題の id と分ける */
  rowId: string;
  task: GanttSource;
  dates: TaskDates;
};

export type GanttGroup = {
  key: string;
  label: string;
  /** 畳んでも数は残す */
  count: number;
  collapsed: boolean;
  items: GanttListItem[];
};

export type GanttLayout = {
  groups: GanttGroup[];
  /** arc の `tasks`。畳んだ群れの課題は入れない */
  tasks: GanttTask[];
  /** arc の `rows`。上から群れの見出しと課題の行 */
  rows: GanttRow[];
  /** 日付が一つも無く描けなかった課題の数 */
  skipped: number;
};

export type LayoutOptions = {
  groupBy: GanttGroupBy;
  /** 畳んだ群れの key */
  collapsed: ReadonlySet<string>;
  /** 選んだ担当者。空なら全件。`UNASSIGNED_MEMBER` で担当の無い課題も入る */
  memberIds: readonly string[];
  members: readonly GanttMember[];
  timeZone: string;
};

const UNASSIGNED_GROUP = UNASSIGNED_MEMBER;

/**
 * 予定の課題を、左の一覧の群れと arc の行に組む。
 *
 * 群れはプロジェクトか担当者。プロジェクトは予定の API が返した順、担当者は名の順に並べ、
 * 担当の無い課題の群れは最後に置く。担当者で群れると、二人で持つ課題は二人の群れの両方に出る。
 * 群れの中は棒の始まり、同じならキーの順。
 */
export function layoutGantt(tasks: readonly GanttSource[], options: LayoutOptions): GanttLayout {
  const groups = new Map<string, GanttGroup>();
  const groupOf = (key: string, label: string) => {
    let group = groups.get(key);
    if (!group) {
      group = { key, label, count: 0, collapsed: options.collapsed.has(key), items: [] };
      groups.set(key, group);
    }
    return group;
  };
  const names = new Map(options.members.map((m) => [m.id, m.username]));
  let skipped = 0;

  for (const task of tasks) {
    if (!matchesMembers(task, options.memberIds, { unassigned: UNASSIGNED_MEMBER })) continue;
    const dates = taskDates(task, options.timeZone);
    if (!dates) {
      skipped += 1;
      continue;
    }
    if (options.groupBy === 'project') {
      groupOf(task.project.id, task.project.name).items.push({ rowId: task.id, task, dates });
      continue;
    }
    if (!task.assignees.length) {
      groupOf(UNASSIGNED_GROUP, '担当なし').items.push({ rowId: task.id, task, dates });
      continue;
    }
    for (const { user_id } of task.assignees) {
      // 選んだ担当者で絞った時は、選んだ者の群れにだけ出す
      if (options.memberIds.length && !options.memberIds.includes(user_id)) continue;
      groupOf(user_id, names.get(user_id) ?? '不明な利用者').items.push({
        rowId: `${task.id}:${user_id}`,
        task,
        dates,
      });
    }
  }

  const ordered = [...groups.values()];
  if (options.groupBy === 'assignee') {
    ordered.sort((a, b) => {
      if (a.key === UNASSIGNED_GROUP) return 1;
      if (b.key === UNASSIGNED_GROUP) return -1;
      return a.label.localeCompare(b.label, 'ja');
    });
  }

  const arcTasks: GanttTask[] = [];
  const arcRows: GanttRow[] = [];
  for (const group of ordered) {
    group.items.sort(
      (a, b) =>
        a.dates.start.localeCompare(b.dates.start) || a.task.seq_key.localeCompare(b.task.seq_key),
    );
    group.count = group.items.length;
    arcRows.push({ kind: 'group' });
    if (group.collapsed) continue;
    for (const item of group.items) {
      arcTasks.push(arcTaskOf(item));
      arcRows.push(arcRowOf(item));
    }
  }
  return { groups: ordered, tasks: arcTasks, rows: arcRows, skipped };
}

/** arc の課題。arc の `end` は境の日なので、最後の日の翌日を渡す */
function arcTaskOf({ rowId, task, dates }: GanttListItem): GanttTask {
  return {
    id: rowId,
    title: task.title,
    progress_pct: task.progress_pct,
    start: dates.start,
    end: addDays(dates.last, 1),
  };
}

/** arc の行と、その行の印 */
function arcRowOf({ rowId, task, dates }: GanttListItem): GanttRow {
  const state = task.status.is_done_state ? 'done' : dates.warn ? 'warn' : null;
  return {
    kind: 'task',
    task_id: rowId,
    hard_deadline: dates.hard,
    state,
    note: warnNote(dates),
    shape: dates.point ? 'point' : 'bar',
  };
}

/** arc の行の id から課題の id へ。担当者で群れた行は `課題の id:利用者の id` */
export function taskIdOfRow(rowId: string): string {
  return rowId.split(':')[0]!;
}

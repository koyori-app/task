import { addDays, dateInZone, diffDays, isWeekend } from '@/lib/calendar/timeline';

/** 1 日に積める見積の上限（時間）。MVP は固定 */
export const DAILY_CAPACITY_HOURS = 6;

export type LoadSource = {
  planned_start_on?: string | null;
  soft_deadline?: string | null;
  estimated_minutes?: number | null;
  logged_minutes: number;
  status: { is_done_state: boolean };
  assignees: { user_id: string; role: string }[];
};

/** `start`〜`end`（両端含む）の平日。土日は非稼働、祝日は見ない */
export function weekdaysBetween(start: string, end: string): string[] {
  const days: string[] = [];
  for (let i = 0; i <= diffDays(start, end); i++) {
    const date = addDays(start, i);
    if (!isWeekend(date)) days.push(date);
  }
  return days;
}

/** ある日に積んだタスクと、そのタスクから載せた時間 */
export type DayLoad<T> = { hours: number; items: { task: T; hours: number }[] };

/**
 * メンバーごと・日ごとの残工数（時間）と、その内訳のタスク。
 *
 * 未完了で、開始予定日と Soft 締切と見積があり、残工数（見積 − 実績）が 1 分以上のタスクを数える。
 * 開始予定日〜Soft 締切（逆転していれば入れ替える）のうち今日以降の平日に残工数を均等に割り、
 * さらに担当者数で均等に割って各担当者に載せる（`role` は見ない）。
 * 平日が残っていなければ 1 日に全部載せる。締切超過なら今日、土日だけの期間ならその直前の平日
 * （今日より前なら今日）。
 */
export function dailyLoadByMember<T extends LoadSource>(
  tasks: readonly T[],
  timeZone: string,
  today: string,
): Map<string, Map<string, DayLoad<T>>> {
  const result = new Map<string, Map<string, DayLoad<T>>>();
  for (const task of tasks) {
    if (task.status.is_done_state || !task.planned_start_on || !task.soft_deadline) continue;
    if (!task.estimated_minutes || !task.assignees.length) continue;
    const remaining = task.estimated_minutes - task.logged_minutes;
    if (remaining < 1) continue;
    const deadline = dateInZone(task.soft_deadline, timeZone);
    const [first, last] =
      task.planned_start_on <= deadline
        ? [task.planned_start_on, deadline]
        : [deadline, task.planned_start_on];
    // 過去の日には積まない
    const weekdays = weekdaysBetween(first > today ? first : today, last);
    let before = addDays(first, -1);
    while (isWeekend(before)) before = addDays(before, -1);
    const days = weekdays.length ? weekdays : [before > today ? before : today];
    const perPerson = remaining / 60 / days.length / task.assignees.length;
    for (const assignee of task.assignees) {
      const byDate = result.get(assignee.user_id) ?? new Map<string, DayLoad<T>>();
      for (const date of days) {
        const day = byDate.get(date) ?? { hours: 0, items: [] };
        day.hours += perPerson;
        day.items.push({ task, hours: perPerson });
        byDate.set(date, day);
      }
      result.set(assignee.user_id, byDate);
    }
  }
  return result;
}

export function isOverCapacity(hours: number): boolean {
  return hours > DAILY_CAPACITY_HOURS;
}

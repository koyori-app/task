import { addDays, dateInZone, diffDays, isWeekend } from '@/lib/calendar/timeline';

/** 1 日に積める見積の上限（時間）。MVP は固定 */
export const DAILY_CAPACITY_HOURS = 6;

export type LoadSource = {
  planned_start_on?: string | null;
  soft_deadline?: string | null;
  estimated_minutes?: number | null;
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
 * メンバーごと・日ごとの見積（時間）と、その内訳のタスク。
 *
 * 主担当で、開始予定日と Soft 締切と見積があり、未完了のタスクだけを数える。
 * `estimated_minutes / 60 ÷ 平日数(開始予定日〜Soft 締切)` を各平日に足す。
 */
export function dailyLoadByMember<T extends LoadSource>(
  tasks: readonly T[],
  timeZone: string,
): Map<string, Map<string, DayLoad<T>>> {
  const result = new Map<string, Map<string, DayLoad<T>>>();
  for (const task of tasks) {
    if (task.status.is_done_state || !task.planned_start_on || !task.soft_deadline) continue;
    if (!task.estimated_minutes) continue;
    const days = weekdaysBetween(task.planned_start_on, dateInZone(task.soft_deadline, timeZone));
    if (!days.length) continue;
    const perDay = task.estimated_minutes / 60 / days.length;
    for (const assignee of task.assignees) {
      if (assignee.role !== 'primary') continue;
      const byDate = result.get(assignee.user_id) ?? new Map<string, DayLoad<T>>();
      for (const date of days) {
        const day = byDate.get(date) ?? { hours: 0, items: [] };
        day.hours += perDay;
        day.items.push({ task, hours: perDay });
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

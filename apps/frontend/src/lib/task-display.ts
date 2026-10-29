import type { LucideIcon } from '@lucide/vue';
import { Signal, SignalHigh, SignalLow, SignalMedium } from '@lucide/vue';
import type { components } from '@/generated/api';

export type ApiPriority = components['schemas']['TaskPriority'];

export const PRIORITY_CONFIG: Record<
  ApiPriority,
  { label: string; color: string; icon: LucideIcon }
> = {
  CriticalFire: { label: '緊急', color: '#dc2626', icon: Signal },
  Critical: { label: '重大', color: '#ef4444', icon: Signal },
  High: { label: '高', color: '#f97316', icon: SignalHigh },
  Medium: { label: '中', color: '#eab308', icon: SignalMedium },
  Low: { label: '低', color: '#6b7280', icon: SignalLow },
  Trivial: { label: '些細', color: '#9ca3af', icon: SignalLow },
};

export function taskSeqKey(projectKey: string, seqId: number): string {
  return `${projectKey}-${seqId}`;
}

export function taskDetailHref(tenant: string, projectKey: string, seqId: number): string {
  return `/${tenant}/projects/${projectKey}/tasks/${taskSeqKey(projectKey, seqId)}`;
}

export function userProfileHref(tenant: string, userId: string): string {
  return `/${tenant}/users/${userId}`;
}

export function taskListHref(tenant: string, projectKey: string): string {
  return `/${tenant}/projects/${projectKey}/tasks`;
}

export function formatTaskDate(iso?: string | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  return d.toLocaleDateString('ja-JP', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function startOfLocalDay(date: Date): Date {
  const result = new Date(date);
  result.setHours(0, 0, 0, 0);
  return result;
}

/**
 * 期限の字と、期限を過ぎているか。
 * 完了の状態にある task（done）は期限切れにしない。終わった仕事を赤くすると、
 * まだ手当てが要ると読まれるためである。期限の日付を出し、相対の字は出さない。
 * 期限より後の日に完了していれば、遅れた日数を `7月10日（4日超過）` のように添える。
 * 遅れは完了した日（completedAt）から数え、今日からは数えない。今日から数えると、
 * 同じ task の遅れが日ごとに増えてしまう。completedAt が無ければ日付だけを出す。
 * 完了かどうかは状態の is_done_state で決め、completedAt は遅れを数えるためだけに使う。
 */
export function formatDeadline(
  iso?: string | null,
  options: { done?: boolean; completedAt?: string | null } = {},
): { label: string; overdue: boolean } | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (options.done) {
    const date = d.toLocaleDateString('ja-JP', { month: 'short', day: 'numeric' });
    if (!options.completedAt) return { label: date, overdue: false };
    const lateDays = Math.round(
      (startOfLocalDay(new Date(options.completedAt)).getTime() - startOfLocalDay(d).getTime()) /
        86400000,
    );
    return { label: lateDays > 0 ? `${date}（${lateDays}日超過）` : date, overdue: false };
  }
  const now = new Date();
  const diff = d.getTime() - now.getTime();
  const overdue = diff < 0;

  const deadlineDay = startOfLocalDay(d);
  const today = startOfLocalDay(now);
  const calendarDays = Math.round((deadlineDay.getTime() - today.getTime()) / 86400000);

  if (overdue) {
    if (calendarDays === 0) return { label: '今日', overdue: true };
    return { label: `${Math.abs(calendarDays)}日超過`, overdue: true };
  }
  if (calendarDays === 0) return { label: '今日', overdue: false };
  if (calendarDays <= 7) return { label: `${calendarDays}日後`, overdue: false };
  return {
    label: d.toLocaleDateString('ja-JP', { month: 'short', day: 'numeric' }),
    overdue: false,
  };
}

/** ISO datetime → calendar date without applying the viewer's timezone. */
export function isoToLocalDateInput(iso?: string | null): string {
  if (!iso) return '';
  const calendarDate = /^(\d{4}-\d{2}-\d{2})(?:T|$)/.exec(iso)?.[1];
  return calendarDate ?? new Date(iso).toISOString().slice(0, 10);
}

/** Calendar date input → stable UTC ISO datetime. */
export function localDateInputToIso(dateValue: string): string {
  return `${dateValue}T00:00:00.000Z`;
}

export function formatProgressPct(value: number): string {
  return `${Math.min(100, Math.max(0, value))}%`;
}

export function clampProgressPct(value: number): number {
  return Math.min(100, Math.max(0, Math.round(value)));
}

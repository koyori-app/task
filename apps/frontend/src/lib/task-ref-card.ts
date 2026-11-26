import type { components } from '@/generated/api';
import { toPlainText, lookup, type MentionTable } from '@/lib/comment-mention';
import { formatDeadline, PRIORITY_CONFIG, type ApiPriority } from '@/lib/task-display';
import { statusFill, type StatusFill } from '@/lib/task-status-icon';

/** 取得せずリンクの対象だけを解決する。開いてからの取得は useTaskRefCard が担う。
 * v-html の容れ物の中だけをカードの起点にする。 */
export const TASK_REF_CARD_SELECTOR = 'a.kfm-task-ref';
export type TaskRefCardTarget = { projectKey: string; seqKey: string; href: string };
export function taskRefCardTarget(
  trigger: Element,
  tenantDisplayId: string,
): TaskRefCardTarget | null {
  const href = trigger.getAttribute('href');
  if (!href) return null;
  const parts = href.split('/');
  const [, tenant, projects, projectKey, tasks, seqKey] = parts;
  if (
    parts.length !== 6 ||
    parts[0] !== '' ||
    tenant !== tenantDisplayId ||
    projects !== 'projects' ||
    tasks !== 'tasks' ||
    !/^[A-Z][A-Z0-9]{1,9}$/.test(projectKey) ||
    !new RegExp(`^${projectKey}-[0-9]+$`).test(seqKey)
  )
    return null;
  return { projectKey, seqKey, href };
}
export function hasTaskRefCards(html: string): boolean {
  return html.includes('kfm-task-ref');
}

type CardTask = Pick<
  components['schemas']['TaskResponse'],
  | 'title'
  | 'description'
  | 'status_id'
  | 'priority'
  | 'assignees'
  | 'labels'
  | 'is_archived'
  | 'soft_deadline'
  | 'hard_deadline'
  | 'completed_at'
> & { mentions?: MentionTable | null };
type CardStatus = components['schemas']['ProjectStatusResponse'];

export interface TaskRefCardStatus {
  name: string;
  color: string;
  done: boolean;
  fill: StatusFill;
}

export function taskRefCardStatus(
  statuses: readonly CardStatus[],
  statusId: string | null | undefined,
): TaskRefCardStatus | null {
  const status = statuses.find((s) => s.id === statusId);
  return status
    ? {
        name: status.name,
        color: status.color,
        done: status.is_done_state,
        fill: statusFill(statuses, status.id),
      }
    : null;
}

/**
 * タスクのカード（TaskRefCard）に渡す中身。説明の中の `KEY-N`（開いてから取得したタスク）と、
 * 一覧の行（手元にあるタスク）の両方がここを通り、同じカードになる。
 */
export function taskRefCardProps(
  task: CardTask,
  statuses: readonly CardStatus[],
  target: { seqKey: string; href: string },
) {
  const status = taskRefCardStatus(statuses, task.status_id);
  const mentions = task.mentions ?? {};
  return {
    ...target,
    state: 'ready' as const,
    title: task.title,
    // メンションの印（`<@利用者ID>`）は id を含むので、札と同じ字に直す
    description: task.description
      ? toPlainText(task.description, (id) => lookup(mentions, id)).trim()
      : '',
    status,
    priority: Object.hasOwn(PRIORITY_CONFIG, task.priority) ? (task.priority as ApiPriority) : null,
    assignees: task.assignees.map((assignee) => assignee.user),
    labels: task.labels,
    archived: task.is_archived,
    deadline: formatDeadline(task.soft_deadline ?? task.hard_deadline, {
      done: status?.done ?? !!task.completed_at,
      completedAt: task.completed_at,
    }),
  };
}

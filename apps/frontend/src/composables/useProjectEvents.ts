import { useQueryClient } from '@tanstack/vue-query';
import { onMounted, onScopeDispose, toValue, watch, type MaybeRefOrGetter } from 'vue';

import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import { TASK_SEARCH_PATH } from '@/lib/api-vue-query';
import { SCHEDULE_PATH } from '@/lib/calendar/schedule';

const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const TASK_RELATIONS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/relations' as const;
// コメントと添付はタスク本体とは別のクエリなので、個別に取り直す
const TASK_COMMENTS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/comments' as const;
const TASK_ATTACHMENTS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments' as const;

const INVALIDATED_PATHS = [
  LIST_TASKS_PATH,
  GET_TASK_PATH,
  TASK_RELATIONS_PATH,
  TASK_COMMENTS_PATH,
  TASK_ATTACHMENTS_PATH,
  TASK_SEARCH_PATH,
  SCHEDULE_PATH,
  ACTIVITIES_PATH,
];

/** 一括更新などで続けて届く合図を 1 回の取り直しにまとめる幅。 */
const INVALIDATE_DELAY_MS = 300;
/** ブラウザが再接続を諦めた後、開き直すまでの間隔。 */
const REOPEN_DELAY_MS = 30_000;

/** 同じプロジェクトでの他の人の変更を受けて、タスクの表示を取り直す。 */
export function useProjectEvents(
  tenantId: MaybeRefOrGetter<TenantUuid | null | undefined>,
  projectId: MaybeRefOrGetter<ProjectUuid | null | undefined>,
) {
  const queryClient = useQueryClient();
  let source: EventSource | null = null;
  let invalidateTimer: ReturnType<typeof setTimeout> | undefined;
  let reopenTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleInvalidate() {
    if (invalidateTimer !== undefined) return;
    invalidateTimer = setTimeout(() => {
      invalidateTimer = undefined;
      for (const path of INVALIDATED_PATHS) {
        void queryClient.invalidateQueries({ queryKey: ['get', path] });
      }
    }, INVALIDATE_DELAY_MS);
  }

  function disconnect() {
    clearTimeout(invalidateTimer);
    invalidateTimer = undefined;
    clearTimeout(reopenTimer);
    reopenTimer = undefined;
    source?.close();
    source = null;
  }

  function connect() {
    disconnect();
    const tenant = toValue(tenantId);
    const project = toValue(projectId);
    if (!tenant || !project) return;

    const apiBase = (import.meta.env.VITE_API_BASE ?? '/api').replace(/\/$/, '');
    const eventSource = new EventSource(
      `${apiBase}/v1/tenants/${tenant}/projects/${project}/events`,
      { withCredentials: true },
    );
    eventSource.addEventListener('tasks', scheduleInvalidate);
    // 購読が成立する前の変更には合図が来ないので、初回も含めて接続のたびに取り直す
    eventSource.addEventListener('open', scheduleInvalidate);
    eventSource.addEventListener('error', () => {
      // デプロイ中の 502 のような非 200 の応答では、ブラウザは再接続せず閉じる
      if (eventSource.readyState === EventSource.CLOSED) {
        reopenTimer = setTimeout(connect, REOPEN_DELAY_MS);
      }
    });
    source = eventSource;
  }

  onMounted(connect);
  watch(() => [toValue(tenantId), toValue(projectId)], connect);
  onScopeDispose(disconnect);
}

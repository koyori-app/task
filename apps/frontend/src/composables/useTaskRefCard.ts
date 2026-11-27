import { computed, toValue, type MaybeRefOrGetter } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { LIST_STATUSES_PATH } from '@/composables/useTaskDetail';
import { fetchClient } from '@/lib/api-vue-query';
import type { TenantUuid } from '@/lib/api-ids';
import { taskRefCardStatus, type TaskRefCardTarget } from '@/lib/task-ref-card';

import { GET_TASK_PATH, fetchTask } from '@/lib/task-query';

/** 開いている対象だけを取得する。成功は60秒共有し、404はnullとして保持する。
 * 取得は中断せず、403・404では以前のタスク情報も隠す。失敗は次に開くと再取得する。
 */
const TASK_REF_CARD_STALE_TIME_MS = 60_000;
export function useTaskRefCard(
  tenantId: MaybeRefOrGetter<TenantUuid | null | undefined>,
  target: MaybeRefOrGetter<TaskRefCardTarget | null>,
) {
  const resolved = useResolvedProjectId(
    () => (toValue(target) ? toValue(tenantId) : null),
    () => toValue(target)?.projectKey ?? '',
  );
  const path = computed(() => ({
    tenant_id: toValue(tenantId)!,
    project_id: resolved.projectId.value!,
  }));
  const params = computed(() => ({ path: { ...path.value, id: toValue(target)?.seqKey ?? '' } }));
  const enabled = computed(
    () => !!toValue(tenantId) && !!resolved.projectId.value && !!toValue(target),
  );
  const taskQuery = useQuery({
    queryKey: computed(() => ['get', GET_TASK_PATH, { params: params.value }]),
    queryFn: () => fetchTask(params.value),
    enabled,
    retry: false,
    staleTime: TASK_REF_CARD_STALE_TIME_MS,
  });
  const statusesQuery = useQuery({
    queryKey: computed(() => ['get', LIST_STATUSES_PATH, { params: { path: path.value } }]),
    queryFn: async () => {
      const { data, error } = await fetchClient.GET(LIST_STATUSES_PATH, {
        params: { path: path.value },
      });
      if (error) throw error;
      return data;
    },
    enabled,
    retry: false,
    staleTime: TASK_REF_CARD_STALE_TIME_MS,
  });
  const state = computed<'loading' | 'ready' | 'unavailable' | 'error'>(() => {
    if (!toValue(target) || !toValue(tenantId)) return 'loading';
    if (resolved.isProjectNotFound.value) return 'unavailable';
    const status = (taskQuery.error.value as { status?: number } | null)?.status;
    if (taskQuery.data.value === null || status === 403 || status === 404) return 'unavailable';
    // アイコンはステータスの一覧から出すので、一覧が届くまで（失敗なら待たずに）ready にしない。
    // ready の後でアイコンが現れると、ID の字を横へ押す
    if (taskQuery.data.value) return statusesQuery.isPending.value ? 'loading' : 'ready';
    if (resolved.isError.value || taskQuery.isError.value) return 'error';
    return 'loading';
  });
  return {
    state,
    task: computed(() => (state.value === 'ready' ? (taskQuery.data.value ?? null) : null)),
    /** カードのアイコン（円の埋まり方）を並び順から出すので、ステータスの一覧ごと返す */
    statuses: computed(() => statusesQuery.data.value ?? []),
    status: computed(() =>
      state.value === 'ready'
        ? taskRefCardStatus(statusesQuery.data.value ?? [], taskQuery.data.value?.status_id)
        : null,
    ),
  };
}

import { computed, toValue, type MaybeRefOrGetter } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { GET_TASK_PATH, LIST_STATUSES_PATH } from '@/composables/useTaskDetail';
import { fetchClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';
import type { TenantUuid } from '@/lib/api-ids';
import type { TaskRefCardTarget } from '@/lib/task-ref-card';

export const TASK_REF_CARD_STALE_TIME_MS = 60_000;
export function useTaskRefCard(
  tenantId: MaybeRefOrGetter<TenantUuid | null | undefined>,
  target: MaybeRefOrGetter<TaskRefCardTarget | null>,
) {
  const resolved = useResolvedProjectId(tenantId, () => toValue(target)?.projectKey ?? '');
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
    queryFn: async () => {
      const { data, error, response } = await fetchClient.GET(GET_TASK_PATH, {
        params: params.value,
      });
      if (response.status === 404) return null;
      if (error) throw Object.assign(new Error(error.message), { status: response.status });
      return data;
    },
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
    if (taskQuery.data.value) return 'ready';
    if (resolved.isError.value || taskQuery.isError.value) return 'error';
    return 'loading';
  });
  return {
    state,
    projectName: computed(
      () =>
        resolved.projectsQuery.data.value?.find(
          (p: components['schemas']['ProjectResponse']) => p.id === resolved.projectId.value,
        )?.name ?? null,
    ),
    task: computed(() => taskQuery.data.value ?? null),
    status: computed(() => {
      const value = statusesQuery.data.value?.find((s) => s.id === taskQuery.data.value?.status_id);
      return value ? { name: value.name, color: value.color, done: value.is_done_state } : null;
    }),
  };
}

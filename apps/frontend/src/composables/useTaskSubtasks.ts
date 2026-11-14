import { computed, ref, toValue, type MaybeRefOrGetter } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';

import { fetchClient } from '@/lib/api-vue-query';
import type { components, paths } from '@/generated/api';

const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const TASK_SEARCH_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search' as const;
// カレンダー（オーバーレイで詳細を開いたまま裏に残る）の帯も同じタスクを持つ
const SCHEDULE_PATH = '/v1/tenants/{tenant_id}/schedule' as const;
export const TASK_RELATIONS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/relations' as const;

type TaskRelationsResponse = components['schemas']['TaskRelationsResponse'];
export type TaskSubtaskFilters = Pick<
  NonNullable<paths[typeof LIST_TASKS_PATH]['get']['parameters']['query']>,
  'status_id' | 'label_id' | 'is_archived'
>;

export type UseTaskSubtasksParams = {
  tenantId: MaybeRefOrGetter<string | null | undefined>;
  projectId: MaybeRefOrGetter<string | null | undefined>;
  /** GET で使う識別子。UUID と PROJECT-42 のどちらでもよい。 */
  taskId: MaybeRefOrGetter<string | null | undefined>;
  /** 作成 payload に入れる親タスク UUID。 */
  taskUuid: MaybeRefOrGetter<string | null | undefined>;
  /** 現在のタスク自身の親。UI の作成は親＋子の1段までに制限する。 */
  parentTaskId: MaybeRefOrGetter<string | null | undefined>;
  enabled?: MaybeRefOrGetter<boolean>;
  /** 一覧の展開では親行と同じ条件を使う。詳細画面では未指定。 */
  filters?: MaybeRefOrGetter<TaskSubtaskFilters>;
  /**
   * 取得の再実行条件。省略すると TanStack の既定（staleTime 0、フォーカス復帰で再取得）に従う。
   * 親行ごとに枝が並ぶ一覧では、フォーカスのたびに全行が取り直されるのを抑えるために使う。
   */
  queryOptions?: MaybeRefOrGetter<TaskSubtaskQueryOptions | undefined>;
};

export type TaskSubtaskQueryOptions = {
  staleTime?: number;
  refetchOnWindowFocus?: boolean;
};

/**
 * 直下のサブタスクと親への導線、およびタイトルだけの子タスク作成をまとめる。
 * 一覧の展開行と詳細画面で同じ取得・失敗・再試行契約を使う。
 */
export function useTaskSubtasks(params: UseTaskSubtasksParams) {
  const queryClient = useQueryClient();
  const tenantId = computed(() => toValue(params.tenantId) ?? '');
  const projectId = computed(() => toValue(params.projectId) ?? '');
  const taskId = computed(() => String(toValue(params.taskId) ?? ''));
  const taskUuid = computed(() => String(toValue(params.taskUuid) ?? ''));
  const enabled = computed(() => (params.enabled === undefined ? true : toValue(params.enabled)));
  const filters = computed(() => toValue(params.filters));
  const canFetch = computed(
    () =>
      enabled.value && !!tenantId.value && !!projectId.value && !!taskId.value && !!taskUuid.value,
  );

  const queryKey = computed(
    () =>
      [
        'get',
        TASK_RELATIONS_PATH,
        {
          params: {
            path: {
              tenant_id: tenantId.value,
              project_id: projectId.value,
              id: taskId.value,
            },
          },
        },
      ] as const,
  );

  // 指定のある項目だけ渡す（undefined で上書きすると既定値が消える）
  const extraOptions = computed(() => {
    const options = toValue(params.queryOptions);
    return {
      ...(options?.staleTime !== undefined && { staleTime: options.staleTime }),
      ...(options?.refetchOnWindowFocus !== undefined && {
        refetchOnWindowFocus: options.refetchOnWindowFocus,
      }),
    };
  });

  const relationsQuery = useQuery(
    computed(() => ({
      ...extraOptions.value,
      queryKey: queryKey.value,
      queryFn: async ({ signal }: { signal: AbortSignal }) => {
        const { data, error } = await fetchClient.GET(TASK_RELATIONS_PATH, {
          params: { path: queryKey.value[2].params.path },
          signal,
        });
        if (error) throw error;
        return data;
      },
      enabled: canFetch.value && !filters.value,
    })),
  );

  const filteredQuery = useQuery(
    computed(() => {
      const path = { tenant_id: tenantId.value, project_id: projectId.value };
      const query = { ...filters.value, parent_task_id: taskUuid.value, limit: 200 };
      return {
        ...extraOptions.value,
        queryKey: ['get', LIST_TASKS_PATH, { params: { path, query } }, 'subtasks'],
        enabled: canFetch.value && !!filters.value,
        queryFn: async ({ signal }: { signal: AbortSignal }) => {
          const tasks: components['schemas']['TaskResponse'][] = [];
          let cursor: string | undefined;
          do {
            const { data, error } = await fetchClient.GET(LIST_TASKS_PATH, {
              params: { path, query: { ...query, cursor } },
              signal,
            });
            if (error) throw error;
            tasks.push(...data.tasks);
            cursor = data.next_cursor ?? undefined;
          } while (cursor);
          // relations と同じ作成順。200件を超える子も最後まで取得する。
          return tasks.reverse();
        },
      };
    }),
  );
  const activeQuery = computed(() => (filters.value ? filteredQuery : relationsQuery));

  const createPending = ref(false);
  const createError = ref<string | null>(null);

  async function createSubtask(title: string, statusId: string): Promise<boolean> {
    const normalizedTitle = title.trim();
    if (
      toValue(params.parentTaskId) ||
      !normalizedTitle ||
      !tenantId.value ||
      !projectId.value ||
      !taskUuid.value ||
      !statusId ||
      createPending.value
    ) {
      return false;
    }

    createPending.value = true;
    createError.value = null;
    const currentRelationsKey = queryKey.value;
    try {
      const { error } = await fetchClient.POST(LIST_TASKS_PATH, {
        params: { path: { tenant_id: tenantId.value, project_id: projectId.value } },
        body: {
          title: normalizedTitle,
          status_id: statusId,
          parent_task_id: taskUuid.value,
          ...(filters.value?.label_id ? { label_ids: [filters.value.label_id] } : {}),
        },
      });
      if (error) throw error;

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: currentRelationsKey, exact: true }),
        queryClient.invalidateQueries({ queryKey: ['get', LIST_TASKS_PATH] }),
        queryClient.invalidateQueries({ queryKey: ['get', GET_TASK_PATH] }),
        queryClient.invalidateQueries({ queryKey: ['get', TASK_SEARCH_PATH] }),
        queryClient.invalidateQueries({ queryKey: ['get', SCHEDULE_PATH] }),
      ]);
      return true;
    } catch {
      createError.value = 'サブタスクを作成できませんでした';
      return false;
    } finally {
      createPending.value = false;
    }
  }

  return {
    relations: computed<TaskRelationsResponse | null>(() => relationsQuery.data.value ?? null),
    subtasks: computed(() =>
      filters.value
        ? (filteredQuery.data.value ?? [])
        : (relationsQuery.data.value?.subtasks ?? []),
    ),
    parentTask: computed(() => relationsQuery.data.value?.parent ?? null),
    loading: computed(() => activeQuery.value.isLoading.value),
    /** 取得結果がまだ無い（未開始・取得中・失敗）。0 件の成功とは区別する */
    pending: computed(() => activeQuery.value.isPending.value),
    error: computed(() => activeQuery.value.isError.value),
    refetch: () => void activeQuery.value.refetch(),
    createPending,
    createError,
    createSubtask,
  };
}

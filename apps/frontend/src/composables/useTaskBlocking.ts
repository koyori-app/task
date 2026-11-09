import { computed, ref, toValue, watch, type MaybeRefOrGetter } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';

import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import { TASK_RELATIONS_PATH } from '@/composables/useTaskSubtasks';
import { fetchClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

const TASK_RELATION_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/relations/{relation_id}' as const;

export type RelationEntry = components['schemas']['RelationEntry'];
export type BlockingRelationType = 'blocks' | 'blocked_by';

export type UseTaskBlockingParams = {
  tenantId: MaybeRefOrGetter<string | null | undefined>;
  projectId: MaybeRefOrGetter<string | null | undefined>;
  /** GET / POST / DELETE で使う識別子。UUID と PROJECT-42 のどちらでもよい。 */
  taskId: MaybeRefOrGetter<string | null | undefined>;
  /** 自分自身の UUID。自己参照の追加を送らないために使う。 */
  taskUuid: MaybeRefOrGetter<string | null | undefined>;
  enabled?: MaybeRefOrGetter<boolean>;
};

function addErrorMessage(status: number | undefined) {
  if (status === 409) return '循環する関係や同じ関係は設定できません';
  if (status === 404) return 'タスクが見つかりません。削除された可能性があります';
  return 'ブロック関係を更新できませんでした';
}

function removeErrorMessage(status: number | undefined) {
  // 相手のタスクではなく関係そのものが無い。別の人が先に外した場合
  if (status === 404) return 'この関係は既に外されています';
  return 'ブロック関係を外せませんでした';
}

/**
 * タスクのブロック関係（blocks / blocked_by）の取得・追加・削除。
 * relations の取得は `useTaskSubtasks` と同じキーなので、キャッシュを共有する。
 */
export function useTaskBlocking(params: UseTaskBlockingParams) {
  const queryClient = useQueryClient();
  const tenantId = computed(() => toValue(params.tenantId) ?? '');
  const projectId = computed(() => toValue(params.projectId) ?? '');
  const taskId = computed(() => String(toValue(params.taskId) ?? ''));
  const taskUuid = computed(() => String(toValue(params.taskUuid) ?? ''));
  const enabled = computed(() => (params.enabled === undefined ? true : toValue(params.enabled)));
  const canFetch = computed(
    () =>
      enabled.value && !!tenantId.value && !!projectId.value && !!taskId.value && !!taskUuid.value,
  );

  const path = computed(() => ({
    tenant_id: tenantId.value,
    project_id: projectId.value,
    id: taskId.value,
  }));

  const relationsQuery = useQuery({
    queryKey: computed(
      () => ['get', TASK_RELATIONS_PATH, { params: { path: path.value } }] as const,
    ),
    queryFn: async ({ signal }) => {
      const { data, error } = await fetchClient.GET(TASK_RELATIONS_PATH, {
        params: { path: path.value },
        signal,
      });
      if (error) throw error;
      return data;
    },
    enabled: canFetch,
  });

  const pending = ref(false);
  const mutationError = ref<string | null>(null);

  // オーバーレイと右ペインは別のタスクへ移っても同じインスタンス
  watch(taskUuid, () => {
    mutationError.value = null;
  });

  // 相手側のタスクの relations も変わるので、キーは prefix で無効化する
  async function invalidateAfterMutation() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', TASK_RELATIONS_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', ACTIVITIES_PATH] }),
    ]);
  }

  async function addRelation(type: BlockingRelationType, targetTaskId: string): Promise<boolean> {
    if (!canFetch.value || !targetTaskId || targetTaskId === taskUuid.value || pending.value) {
      return false;
    }
    pending.value = true;
    mutationError.value = null;
    try {
      const { error, response } = await fetchClient.POST(TASK_RELATIONS_PATH, {
        params: { path: path.value },
        body: { type, target_task_id: targetTaskId },
      });
      if (error) {
        mutationError.value = addErrorMessage(response.status);
        return false;
      }
      await invalidateAfterMutation();
      return true;
    } catch {
      mutationError.value = 'ブロック関係を更新できませんでした';
      return false;
    } finally {
      pending.value = false;
    }
  }

  async function removeRelation(relationId: string): Promise<boolean> {
    if (!canFetch.value || !relationId || pending.value) return false;
    pending.value = true;
    mutationError.value = null;
    try {
      const { error, response } = await fetchClient.DELETE(TASK_RELATION_PATH, {
        params: { path: { ...path.value, relation_id: relationId } },
      });
      if (error) {
        mutationError.value = removeErrorMessage(response.status);
        // 既に外されていた場合、手元の一覧には消えたはずの行が残っている。取り直して揃える
        if (response.status === 404) {
          await queryClient.invalidateQueries({ queryKey: ['get', TASK_RELATIONS_PATH] });
        }
        return false;
      }
      await invalidateAfterMutation();
      return true;
    } catch {
      mutationError.value = 'ブロック関係を外せませんでした';
      return false;
    } finally {
      pending.value = false;
    }
  }

  return {
    blocks: computed<RelationEntry[]>(() => relationsQuery.data.value?.blocks ?? []),
    blockedBy: computed<RelationEntry[]>(() => relationsQuery.data.value?.blocked_by ?? []),
    loading: computed(() => relationsQuery.isLoading.value),
    error: computed(() => relationsQuery.isError.value),
    refetch: () => void relationsQuery.refetch(),
    mutating: computed(() => pending.value),
    mutationError,
    addRelation,
    removeRelation,
  };
}

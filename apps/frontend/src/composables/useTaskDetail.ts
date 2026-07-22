import { computed, ref, watch, type MaybeRefOrGetter, toValue } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';

import type { EditableField } from '@/components/tasks/editable-field';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { fetchClient, apiClient } from '@/lib/api-vue-query';
import { clampProgressPct, localDateInputToIso, taskListHref } from '@/lib/task-display';
import type { components } from '@/generated/api';

const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const LIST_STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

type TaskDetail = components['schemas']['TaskDetailResponse'];
type UpdateTaskRequest = components['schemas']['UpdateTaskRequest'];
type MutatingField = EditableField | 'status_id';

export interface UseTaskDetailParams {
  /** ルートの tenant セグメント（表示ID）。テナント UUID 解決に使う */
  tenantDisplayId: MaybeRefOrGetter<string>;
  /** プロジェクトの key。プロジェクト UUID 解決に使う */
  projectKey: MaybeRefOrGetter<string>;
  /** タスク識別子（URL と同じ seq key 形式。例: "ENG-42"）。空文字なら未取得 */
  taskId: MaybeRefOrGetter<string>;
  /**
   * 削除成功後に呼ばれる。省略時は一覧へ遷移する。
   * 分割ビューのペインでは「ペインを閉じる」を渡す。
   */
  onAfterDelete?: (listHref: string) => void;
}

/**
 * タスク詳細（取得・楽観更新・各フィールド保存・削除）のロジック。
 * フルページ詳細（@taskId/+Page.vue）と一覧の分割ビュー右ペインの両方から使う。
 * 表示は {@link TaskDetailHub} に委譲し、この composable は状態と操作だけを返す。
 */
export function useTaskDetail(params: UseTaskDetailParams) {
  const tenantDisplayId = computed(() => String(toValue(params.tenantDisplayId) ?? ''));
  const projectKey = computed(() => String(toValue(params.projectKey) ?? ''));
  const taskId = computed(() => String(toValue(params.taskId) ?? ''));
  const onAfterDelete = params.onAfterDelete ?? ((href: string) => void navigate(href));

  const queryClient = useQueryClient();

  const {
    tenantId,
    isTenantNotFound,
    isResolving: isTenantResolving,
    isError: isTenantResolveError,
  } = useResolvedTenantId(tenantDisplayId);
  const {
    projectId,
    isProjectNotFound,
    isResolving: isProjectResolving,
    isError: isProjectResolveError,
  } = useResolvedProjectId(tenantId, projectKey);

  const statusError = ref<string | null>(null);
  const deleteError = ref<string | null>(null);
  const fieldErrors = ref<Partial<Record<EditableField, string>>>({});
  const selectedStatusId = ref('');
  const optimisticTask = ref<Partial<TaskDetail>>({});
  const pendingFieldRevisions = ref<Partial<Record<MutatingField, number>>>({});
  const appliedFieldRevisions: Partial<Record<MutatingField, number>> = {};
  let nextMutationRevision = 0;

  const taskQueryKey = computed(
    () =>
      [
        'get',
        GET_TASK_PATH,
        {
          params: {
            path: {
              tenant_id: tenantId.value!,
              project_id: projectId.value!,
              id: taskId.value,
            },
          },
        },
      ] as const,
  );

  const taskQuery = useQuery({
    queryKey: taskQueryKey,
    queryFn: async ({ signal }) => {
      const { data, error, response } = await fetchClient.GET(GET_TASK_PATH, {
        params: {
          path: {
            tenant_id: tenantId.value!,
            project_id: projectId.value!,
            id: taskId.value,
          },
        },
        signal,
      });
      if (response.status === 404) return null;
      if (error) throw error;
      return data;
    },
    enabled: computed(() => !!tenantId.value && !!projectId.value && !!taskId.value),
  });

  const statusesQuery = useQuery({
    queryKey: computed(() => [
      'get',
      LIST_STATUSES_PATH,
      { params: { path: { tenant_id: tenantId.value!, project_id: projectId.value! } } },
    ]),
    queryFn: async ({ signal }) => {
      const { data, error } = await fetchClient.GET(LIST_STATUSES_PATH, {
        params: { path: { tenant_id: tenantId.value!, project_id: projectId.value! } },
        signal,
      });
      if (error) throw error;
      return data;
    },
    enabled: computed(() => !!tenantId.value && !!projectId.value),
  });

  watch(
    () => taskQuery.data.value?.status_id,
    (statusId) => {
      if (statusId) selectedStatusId.value = statusId;
    },
    { immediate: true },
  );

  const displayTask = computed(() => {
    const base = taskQuery.data.value;
    if (!base) return null;
    return { ...base, ...optimisticTask.value };
  });

  const fieldUpdating = computed(() => {
    const pending = pendingFieldRevisions.value;
    return {
      title: pending.title !== undefined,
      description: pending.description !== undefined,
      progress_pct: pending.progress_pct !== undefined,
      soft_deadline: pending.soft_deadline !== undefined,
      hard_deadline: pending.hard_deadline !== undefined,
    };
  });

  const statusUpdating = computed(() => pendingFieldRevisions.value.status_id !== undefined);

  const updateTaskMutation = apiClient.useMutation('put', GET_TASK_PATH);

  const listHref = computed(() => taskListHref(tenantDisplayId.value, projectKey.value));

  const deleteTaskMutation = apiClient.useMutation('delete', GET_TASK_PATH, {
    onSuccess: async () => {
      deleteError.value = null;
      queryClient.removeQueries({ queryKey: taskQueryKey.value, exact: true });
      await queryClient.invalidateQueries({
        queryKey: ['get', LIST_TASKS_PATH],
      });
      onAfterDelete(listHref.value);
    },
    onError: () => {
      deleteError.value = 'タスクの削除に失敗しました';
    },
  });

  function rollbackOptimistic(field: MutatingField, revision: number) {
    if (pendingFieldRevisions.value[field] !== revision) return;

    const nextOptimistic = { ...optimisticTask.value };
    delete nextOptimistic[field];
    optimisticTask.value = nextOptimistic;
    const nextPending = { ...pendingFieldRevisions.value };
    delete nextPending[field];
    pendingFieldRevisions.value = nextPending;

    if (field === 'status_id') {
      statusError.value = 'ステータスの更新に失敗しました';
      const currentStatusId = taskQuery.data.value?.status_id;
      if (currentStatusId) selectedStatusId.value = currentStatusId;
      return;
    }
    fieldErrors.value = {
      ...fieldErrors.value,
      [field]: '更新に失敗しました',
    };
  }

  function applyMutationSuccess(field: MutatingField, revision: number, data: TaskDetail) {
    const appliedRevision = appliedFieldRevisions[field] ?? 0;
    if (revision > appliedRevision) {
      appliedFieldRevisions[field] = revision;
      queryClient.setQueryData<TaskDetail | null>(taskQueryKey.value, (current) =>
        current ? { ...current, [field]: data[field] } : data,
      );
    }

    if (pendingFieldRevisions.value[field] !== revision) return;

    const nextOptimistic = { ...optimisticTask.value };
    delete nextOptimistic[field];
    optimisticTask.value = nextOptimistic;
    const nextPending = { ...pendingFieldRevisions.value };
    delete nextPending[field];
    pendingFieldRevisions.value = nextPending;

    if (field === 'status_id') {
      statusError.value = null;
      if (data.status_id) selectedStatusId.value = data.status_id;
    } else {
      fieldErrors.value = { ...fieldErrors.value, [field]: undefined };
    }
  }

  function mutateTask(
    body: UpdateTaskRequest,
    optimistic: Partial<TaskDetail>,
    field: MutatingField,
  ) {
    if (!tenantId.value || !projectId.value || !taskId.value) return;

    const revision = ++nextMutationRevision;
    optimisticTask.value = { ...optimisticTask.value, ...optimistic };
    pendingFieldRevisions.value = { ...pendingFieldRevisions.value, [field]: revision };
    if (field === 'status_id') statusError.value = null;
    else fieldErrors.value = { ...fieldErrors.value, [field]: undefined };

    updateTaskMutation.mutate(
      {
        params: {
          path: {
            tenant_id: tenantId.value,
            project_id: projectId.value,
            id: taskId.value,
          },
        },
        body,
      },
      {
        onSuccess: (data: TaskDetail) => {
          applyMutationSuccess(field, revision, data);
          void queryClient.invalidateQueries({ queryKey: ['get', LIST_TASKS_PATH] });
        },
        onError: () => rollbackOptimistic(field, revision),
      },
    );
  }

  function onStatusChange(nextStatusId: string) {
    if (!taskQuery.data.value) return;
    if (nextStatusId === taskQuery.data.value.status_id) return;

    selectedStatusId.value = nextStatusId;
    mutateTask({ status_id: nextStatusId }, { status_id: nextStatusId }, 'status_id');
  }

  function onSaveTitle(value: string) {
    const current = taskQuery.data.value;
    if (!current || value === current.title) return;
    mutateTask({ title: value }, { title: value }, 'title');
  }

  function onSaveDescription(value: string | null) {
    const current = taskQuery.data.value;
    if (!current) return;
    const normalized = value?.trim() ?? '';
    const currentDescription = current.description ?? '';
    if (normalized === currentDescription) return;

    const body: UpdateTaskRequest = normalized
      ? { description: normalized }
      : { clear_description: true };
    mutateTask(body, { description: normalized || null }, 'description');
  }

  function onSaveProgressPct(value: number) {
    const current = taskQuery.data.value;
    if (!current) return;
    const next = clampProgressPct(value);
    if (next === current.progress_pct) return;
    mutateTask({ progress_pct: next }, { progress_pct: next }, 'progress_pct');
  }

  function onSaveSoftDeadline(value: string | null) {
    const current = taskQuery.data.value;
    if (!current) return;

    if (!value) {
      if (!current.soft_deadline) return;
      mutateTask({ clear_soft_deadline: true }, { soft_deadline: null }, 'soft_deadline');
      return;
    }

    const iso = localDateInputToIso(value);
    if (iso === current.soft_deadline) return;
    mutateTask({ soft_deadline: iso }, { soft_deadline: iso }, 'soft_deadline');
  }

  function onSaveHardDeadline(value: string | null) {
    const current = taskQuery.data.value;
    if (!current) return;

    if (!value) {
      if (!current.hard_deadline) return;
      mutateTask({ clear_hard_deadline: true }, { hard_deadline: null }, 'hard_deadline');
      return;
    }

    const iso = localDateInputToIso(value);
    if (iso === current.hard_deadline) return;
    mutateTask({ hard_deadline: iso }, { hard_deadline: iso }, 'hard_deadline');
  }

  function confirmDelete() {
    if (!tenantId.value || !projectId.value || !taskId.value) return;
    deleteError.value = null;
    deleteTaskMutation.mutate({
      params: {
        path: {
          tenant_id: tenantId.value,
          project_id: projectId.value,
          id: taskId.value,
        },
      },
    });
  }

  const isLoading = computed(
    () =>
      isTenantResolving.value ||
      isProjectResolving.value ||
      taskQuery.isLoading.value ||
      statusesQuery.isLoading.value,
  );

  const isError = computed(
    () =>
      isTenantResolveError.value ||
      isProjectResolveError.value ||
      taskQuery.isError.value ||
      statusesQuery.isError.value,
  );

  const isNotFound = computed(
    () =>
      isTenantNotFound.value ||
      isProjectNotFound.value ||
      (taskQuery.isSuccess.value && taskQuery.data.value === null),
  );

  return {
    // 解決済み ID（消費側で必要になれば使う）
    tenantId,
    projectId,
    // Hub バインド用
    displayTask,
    statuses: computed(() => statusesQuery.data.value ?? []),
    selectedStatusId,
    statusUpdating,
    statusError,
    fieldUpdating,
    fieldErrors,
    isLoading,
    isNotFound,
    isError,
    // 操作
    onStatusChange,
    onSaveTitle,
    onSaveDescription,
    onSaveProgressPct,
    onSaveSoftDeadline,
    onSaveHardDeadline,
    // 削除
    deleteError,
    deletePending: computed(() => deleteTaskMutation.isPending.value),
    confirmDelete,
    // ナビゲーション
    listHref,
  };
}

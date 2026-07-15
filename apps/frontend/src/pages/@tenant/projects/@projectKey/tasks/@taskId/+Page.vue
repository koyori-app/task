<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { usePageContext } from 'vike-vue/usePageContext';

import TaskDetailHub from '@/components/tasks/TaskDetailHub.vue';
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

type EditableField = 'title' | 'description' | 'progress_pct' | 'soft_deadline' | 'hard_deadline';

const pageContext = usePageContext();
const queryClient = useQueryClient();

const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));
const taskId = computed(() => String(pageContext.routeParams.taskId ?? ''));

const statusError = ref<string | null>(null);
const fieldErrors = ref<Partial<Record<EditableField, string>>>({});
const selectedStatusId = ref('');
const optimisticTask = ref<Partial<TaskDetail>>({});
const updatingField = ref<EditableField | 'status_id' | null>(null);

const {
  projectId,
  isProjectNotFound,
  isResolving: isProjectResolving,
  isError: isProjectResolveError,
} = useResolvedProjectId(tenantId, projectKey);

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
  const field = updatingField.value;
  if (!field || field === 'status_id') return {};
  return { [field]: updateTaskMutation.isPending.value };
});

const updateTaskMutation = apiClient.useMutation('put', GET_TASK_PATH, {
  onSuccess: (data: TaskDetail) => {
    statusError.value = null;
    fieldErrors.value = {};
    optimisticTask.value = {};
    updatingField.value = null;
    queryClient.setQueryData(taskQueryKey.value, data);
    if (data.status_id) selectedStatusId.value = data.status_id;
    queryClient.invalidateQueries({
      queryKey: ['get', LIST_TASKS_PATH],
    });
  },
});

function rollbackOptimistic(
  field: EditableField | 'status_id',
  previous: TaskDetail | null | undefined,
) {
  optimisticTask.value = {};
  updatingField.value = null;
  if (field === 'status_id') {
    statusError.value = 'ステータスの更新に失敗しました';
    if (previous?.status_id) selectedStatusId.value = previous.status_id;
    return;
  }
  fieldErrors.value = {
    ...fieldErrors.value,
    [field]: '更新に失敗しました',
  };
}

function mutateTask(
  body: UpdateTaskRequest,
  optimistic: Partial<TaskDetail>,
  field: EditableField | 'status_id',
) {
  if (!tenantId.value || !projectId.value || !taskId.value) return;

  const previous = taskQuery.data.value;
  optimisticTask.value = { ...optimisticTask.value, ...optimistic };
  updatingField.value = field;
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
      onError: () => rollbackOptimistic(field, previous),
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

const listHref = computed(() => taskListHref(tenantDisplayId.value, projectKey.value));

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
</script>

<template>
  <TaskDetailHub
    :task="displayTask"
    :project-key="projectKey"
    :statuses="statusesQuery.data.value ?? []"
    :status-id="selectedStatusId"
    :status-updating="updatingField === 'status_id' && updateTaskMutation.isPending.value"
    :status-error="statusError"
    :field-updating="fieldUpdating"
    :field-errors="fieldErrors"
    :loading="isLoading"
    :not-found="isNotFound"
    :error="isError"
    @update:status-id="onStatusChange"
    @save:title="onSaveTitle"
    @save:description="onSaveDescription"
    @save:progress_pct="onSaveProgressPct"
    @save:soft_deadline="onSaveSoftDeadline"
    @save:hard_deadline="onSaveHardDeadline"
  >
    <template #breadcrumb>
      <a :href="listHref" class="text-primary hover:underline">タスク一覧</a>
      <span aria-hidden="true">/</span>
    </template>
    <template #footer>
      <p class="text-xs text-muted-foreground">
        このページはタスク詳細ハブの増分2です。タイトル・説明・進捗・期限をインライン編集できます。
      </p>
    </template>
  </TaskDetailHub>
</template>

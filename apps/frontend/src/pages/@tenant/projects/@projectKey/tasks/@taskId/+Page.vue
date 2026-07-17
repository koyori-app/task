<script setup lang="ts">
import { computed, inject, ref, watch } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';

import TaskDetailHub from '@/components/tasks/TaskDetailHub.vue';
import type { EditableField } from '@/components/tasks/editable-field';
import { Button } from '@/components/ui/button';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { fetchClient, apiClient } from '@/lib/api-vue-query';
import { clampProgressPct, localDateInputToIso, taskListHref } from '@/lib/task-display';
import type { components } from '@/generated/api';

const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const LIST_STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

const navigateAfterDelete = inject<(href: string) => void>('navigateAfterDelete', (href) => {
  void navigate(href);
});

type TaskDetail = components['schemas']['TaskDetailResponse'];
type UpdateTaskRequest = components['schemas']['UpdateTaskRequest'];

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
const deleteError = ref<string | null>(null);
const fieldErrors = ref<Partial<Record<EditableField, string>>>({});
const selectedStatusId = ref('');
const optimisticTask = ref<Partial<TaskDetail>>({});
type MutatingField = EditableField | 'status_id';
const pendingFieldRevisions = ref<Partial<Record<MutatingField, number>>>({});
const appliedFieldRevisions: Partial<Record<MutatingField, number>> = {};
let nextMutationRevision = 0;
const deleteDialogRef = ref<HTMLDialogElement | null>(null);

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
  const pending = pendingFieldRevisions.value;
  return {
    title: pending.title !== undefined,
    description: pending.description !== undefined,
    progress_pct: pending.progress_pct !== undefined,
    soft_deadline: pending.soft_deadline !== undefined,
    hard_deadline: pending.hard_deadline !== undefined,
  };
});

const updateTaskMutation = apiClient.useMutation('put', GET_TASK_PATH);

const deleteTaskMutation = apiClient.useMutation('delete', GET_TASK_PATH, {
  onSuccess: async () => {
    deleteError.value = null;
    closeDeleteDialog();
    queryClient.removeQueries({ queryKey: taskQueryKey.value, exact: true });
    await queryClient.invalidateQueries({
      queryKey: ['get', LIST_TASKS_PATH],
    });
    navigateAfterDelete(listHref.value);
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
  field: EditableField | 'status_id',
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
        queryClient.invalidateQueries({ queryKey: ['get', LIST_TASKS_PATH] });
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

const listHref = computed(() => taskListHref(tenantDisplayId.value, projectKey.value));

function openDeleteDialog() {
  deleteError.value = null;
  deleteDialogRef.value?.showModal();
}

function closeDeleteDialog() {
  deleteDialogRef.value?.close();
}

function onDeleteDialogCancel(event: Event) {
  if (deleteTaskMutation.isPending.value) {
    event.preventDefault();
    return;
  }
  event.preventDefault();
  closeDeleteDialog();
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
</script>

<template>
  <TaskDetailHub
    :task="displayTask"
    :project-key="projectKey"
    :statuses="statusesQuery.data.value ?? []"
    :status-id="selectedStatusId"
    :status-updating="pendingFieldRevisions.status_id !== undefined"
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
    :delete-disabled="deleteTaskMutation.isPending.value"
    @delete-request="openDeleteDialog"
  >
    <template #breadcrumb>
      <a :href="listHref" class="text-primary hover:underline">タスク一覧</a>
      <span aria-hidden="true">/</span>
    </template>
    <template #header-actions>
      <dialog
        ref="deleteDialogRef"
        class="fixed top-1/2 left-1/2 w-[calc(100%-2rem)] max-w-md -translate-x-1/2 -translate-y-1/2 rounded-lg border bg-background p-6 shadow-lg backdrop:bg-black/50 open:flex open:flex-col open:gap-4"
        aria-labelledby="delete-task-dialog-title"
        @cancel="onDeleteDialogCancel"
      >
        <h2 id="delete-task-dialog-title" class="text-lg font-semibold">タスクを削除しますか？</h2>
        <p class="text-sm text-muted-foreground">
          「{{ displayTask?.title }}」を削除します。この操作は取り消せません。
        </p>
        <p v-if="deleteError" class="text-sm text-destructive">{{ deleteError }}</p>
        <div class="flex justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            :disabled="deleteTaskMutation.isPending.value"
            @click="closeDeleteDialog"
          >
            キャンセル
          </Button>
          <Button
            type="button"
            variant="destructive"
            :disabled="deleteTaskMutation.isPending.value"
            @click="confirmDelete"
          >
            {{ deleteTaskMutation.isPending.value ? '削除中…' : '削除する' }}
          </Button>
        </div>
      </dialog>
    </template>
    <template #footer>
      <p class="text-xs text-muted-foreground">
        このページはタスク詳細ハブの増分2です。タイトル・説明・進捗・期限をインライン編集できます。
      </p>
    </template>
  </TaskDetailHub>
</template>

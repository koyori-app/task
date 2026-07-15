<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { usePageContext } from 'vike-vue/usePageContext';

import TaskDetailHub from '@/components/tasks/TaskDetailHub.vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { fetchClient, apiClient } from '@/lib/api-vue-query';
import { taskListHref } from '@/lib/task-display';
import type { components } from '@/generated/api';

const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const LIST_STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

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
const selectedStatusId = ref('');

const {
  projectId,
  isProjectNotFound,
  isResolving: isProjectResolving,
  isError: isProjectResolveError,
} = useResolvedProjectId(tenantId, projectKey);

const taskQuery = useQuery({
  queryKey: computed(() => [
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
  ]),
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

const updateStatusMutation = apiClient.useMutation('put', GET_TASK_PATH, {
  onSuccess: (data: components['schemas']['TaskDetailResponse']) => {
    statusError.value = null;
    queryClient.setQueryData(
      [
        'get',
        GET_TASK_PATH,
        {
          params: {
            path: {
              tenant_id: tenantId.value,
              project_id: projectId.value,
              id: taskId.value,
            },
          },
        },
      ],
      data,
    );
    queryClient.invalidateQueries({
      queryKey: ['get', LIST_TASKS_PATH],
    });
  },
  onError: () => {
    statusError.value = 'ステータスの更新に失敗しました';
    if (taskQuery.data.value?.status_id) {
      selectedStatusId.value = taskQuery.data.value.status_id;
    }
  },
});

function onStatusChange(nextStatusId: string) {
  if (!tenantId.value || !projectId.value || !taskId.value) return;
  if (nextStatusId === taskQuery.data.value?.status_id) return;

  selectedStatusId.value = nextStatusId;
  statusError.value = null;

  const body: components['schemas']['UpdateTaskRequest'] = {
    status_id: nextStatusId,
  };

  updateStatusMutation.mutate({
    params: {
      path: {
        tenant_id: tenantId.value,
        project_id: projectId.value,
        id: taskId.value,
      },
    },
    body,
  });
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
    :task="taskQuery.data.value ?? null"
    :project-key="projectKey"
    :statuses="statusesQuery.data.value ?? []"
    :status-id="selectedStatusId"
    :status-updating="updateStatusMutation.isPending.value"
    :status-error="statusError"
    :loading="isLoading"
    :not-found="isNotFound"
    :error="isError"
    @update:status-id="onStatusChange"
  >
    <template #breadcrumb>
      <a :href="listHref" class="text-primary hover:underline">タスク一覧</a>
      <span aria-hidden="true">/</span>
    </template>
    <template #footer>
      <p class="text-xs text-muted-foreground">
        このページはタスク詳細ハブの増分1です。編集・コメント・リンク・工数などは今後ここに追加されます。
      </p>
    </template>
  </TaskDetailHub>
</template>

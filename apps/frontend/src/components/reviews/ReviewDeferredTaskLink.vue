<script setup lang="ts">
import { computed } from 'vue';
import { Badge } from '@koyori-app/ui-vue';

import { apiClient } from '@/lib/api-vue-query';
import { taskDetailHref, taskSeqKey } from '@/lib/task-display';

const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;

const props = defineProps<{
  tenantId: string;
  tenantSlug: string;
  projectId: string;
  projectKey: string;
  /** 繰り延べ先のタスクの UUID */
  taskId: string;
}>();

// 指摘が持つのは UUID だけなので、番号（TASK-512）はタスクを引いて知る。
// 取得を待つ間と失敗（削除済みは 404）は何も出さない
const taskQuery = apiClient.useQuery(
  'get',
  GET_TASK_PATH,
  {
    params: {
      path: { tenant_id: props.tenantId, project_id: props.projectId, id: props.taskId },
    },
  },
  { retry: false, staleTime: 60_000 },
);

const seqId = computed(() => taskQuery.data.value?.seq_id ?? null);
</script>

<template>
  <a
    v-if="seqId !== null"
    class="shrink-0 rounded-full"
    :href="taskDetailHref(tenantSlug, projectKey, seqId)"
    :aria-label="`繰り延べ先のタスク ${taskSeqKey(projectKey, seqId)}`"
    data-testid="deferred-task"
  >
    <Badge :label="taskSeqKey(projectKey, seqId)" tone="info" size="sm" />
  </a>
</template>

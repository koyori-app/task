<script setup lang="ts">
import { useTemplateRef } from 'vue';

import TaskRow from '@/components/tasks/TaskRow.vue';
import TaskRowHoverCard from '@/components/tasks/TaskRowHoverCard.vue';
import type { components } from '@/generated/api';

type TaskResponse = components['schemas']['TaskResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

/**
 * 詳細の「似ているタスク」。サブタスクと同じ行（TaskRow）で並べ、ホバーでタスクのカードを出す。
 * 作成ダイアログの候補（SimilarTasksList）は ID と題だけの別の形のまま。
 */
defineProps<{
  tasks: components['schemas']['SimilarTaskHit'][];
  statuses: StatusResponse[];
  tenantDisplayId: string;
  projectKey: string;
  rowPending?: Record<string, unknown>;
  rowErrors?: Record<string, string | undefined>;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
  'update:status': [task: TaskResponse, statusId: string];
}>();

const listEl = useTemplateRef<HTMLElement>('list');
</script>

<template>
  <section v-if="tasks.length" class="border-t pt-5" data-similar-tasks>
    <h2 class="mb-1 flex min-h-8 items-center text-sm font-medium text-muted-foreground">
      似ているタスク
    </h2>
    <ul ref="list" class="@container divide-y divide-border/60">
      <li v-for="task in tasks" :key="task.id">
        <TaskRow
          :task="task"
          :project-key="projectKey"
          :tenant-display-id="tenantDisplayId"
          :statuses="statuses"
          :pending="!!rowPending?.[task.id]"
          :error="rowErrors?.[task.id]"
          @open="emit('open', $event)"
          @update:status="(row, statusId) => emit('update:status', row, statusId)"
        />
      </li>
    </ul>
    <TaskRowHoverCard
      :container="listEl"
      :tasks="tasks"
      :statuses="statuses"
      :project-key="projectKey"
      :tenant-display-id="tenantDisplayId"
    />
  </section>
</template>

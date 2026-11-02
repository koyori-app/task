<script setup lang="ts">
import TaskTitleLink from '@/components/tasks/TaskTitleLink.vue';
import type { components } from '@/generated/api';
import { taskSeqKey } from '@/lib/task-display';

defineProps<{
  tasks: components['schemas']['SimilarTaskHit'][];
  tenantDisplayId: string;
  projectKey: string;
}>();

const emit = defineEmits<{
  select: [seqId: number];
}>();
</script>

<template>
  <section v-if="tasks.length" data-similar-tasks>
    <h2 class="mb-1 text-sm font-medium text-muted-foreground">似ているタスク</h2>
    <ul class="flex flex-col gap-1">
      <li v-for="task in tasks" :key="task.id" class="flex min-w-0 items-baseline gap-2">
        <span class="shrink-0 font-mono text-xs text-muted-foreground">
          {{ taskSeqKey(projectKey, task.seq_id) }}
        </span>
        <TaskTitleLink
          :tenant-display-id="tenantDisplayId"
          :project-key="projectKey"
          :seq-id="task.seq_id"
          :title="task.title"
          @select="emit('select', $event)"
        />
      </li>
    </ul>
  </section>
</template>

<script setup lang="ts">
import { HoverCard } from '@koyori-app/ui-vue';
import { computed, ref } from 'vue';

import TaskRefCard from '@/components/tasks/TaskRefCard.vue';
import type { components } from '@/generated/api';
import { taskDetailHref, taskSeqKey } from '@/lib/task-display';
import { taskRefCardProps } from '@/lib/task-ref-card';

type TaskResponse = components['schemas']['TaskResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

/** 一覧の行の、題と ID のブロック。ここへのホバーとフォーカスでカードを出す */
const TASK_ROW_CARD_SELECTOR = '[data-task-row-card]';

/**
 * 一覧の行のタスクのホバーカード（TASK-590）。説明の中の `KEY-N` と同じカードを出す。
 * 行のタスクは手元にあるので取得しない。起点の `data-task-id` から引く。
 */
const props = defineProps<{
  container: HTMLElement | null;
  tasks: TaskResponse[];
  statuses: StatusResponse[];
  projectKey: string;
  tenantDisplayId: string;
}>();

const taskId = ref<string | null>(null);
function onOpenChange(open: boolean, trigger: HTMLElement) {
  taskId.value = open ? (trigger.dataset.taskId ?? null) : null;
}

const task = computed(() => props.tasks.find((t) => t.id === taskId.value) ?? null);
const seqKey = computed(() =>
  task.value ? taskSeqKey(props.projectKey, task.value.seq_id) : null,
);
const cardProps = computed(() =>
  task.value && seqKey.value
    ? taskRefCardProps(task.value, props.statuses, {
        seqKey: seqKey.value,
        href: taskDetailHref(props.tenantDisplayId, props.projectKey, task.value.seq_id),
      })
    : null,
);
</script>

<template>
  <HoverCard
    :container="container"
    :selector="TASK_ROW_CARD_SELECTOR"
    :label="seqKey ? `${seqKey} のタスク` : 'タスクの情報'"
    :on-open-change="onOpenChange"
  >
    <template #content><TaskRefCard v-if="cardProps" v-bind="cardProps" /></template>
  </HoverCard>
</template>

<script setup lang="ts">
import { HoverCard } from '@koyori-app/ui-vue';
import { computed, ref } from 'vue';
import TaskRefCard from '@/components/tasks/TaskRefCard.vue';
import { useTaskRefCard } from '@/composables/useTaskRefCard';
import type { TenantUuid } from '@/lib/api-ids';
import {
  TASK_REF_CARD_SELECTOR,
  taskRefCardTarget,
  type TaskRefCardTarget,
} from '@/lib/task-ref-card';
import { formatDeadline, PRIORITY_CONFIG, type ApiPriority } from '@/lib/task-display';
/** 本文のリンクを委譲で受ける。開くまで一覧も取得せず、閉じた後の取得は完了させる。 */
const props = defineProps<{
  container: HTMLElement | null;
  tenantId: string;
  tenantDisplayId: string;
}>();
const target = ref<TaskRefCardTarget | null>(null);
function onOpenChange(open: boolean, trigger: HTMLElement) {
  target.value = open ? taskRefCardTarget(trigger, props.tenantDisplayId) : null;
}
const card = useTaskRefCard(() => (target.value ? (props.tenantId as TenantUuid) : null), target);
const label = computed(() => (target.value ? `${target.value.seqKey} のタスク` : 'タスクの情報'));
const cardProps = computed(() => {
  if (!target.value) return null;
  const task = card.task.value;
  const priority = task?.priority;
  return {
    ...target.value,
    state: card.state.value,
    projectName: card.projectName.value,
    title: task?.title,
    status: card.status.value,
    priority:
      priority && Object.hasOwn(PRIORITY_CONFIG, priority) ? (priority as ApiPriority) : null,
    assignees: task?.assignees.map((a) => a.user),
    archived: task?.is_archived,
    deadline: formatDeadline(task?.soft_deadline ?? task?.hard_deadline, {
      done: card.status.value?.done ?? !!task?.completed_at,
      completedAt: task?.completed_at,
    }),
  };
});
</script>
<template>
  <HoverCard
    :container="container"
    :selector="TASK_REF_CARD_SELECTOR"
    :label="label"
    :on-open-change="onOpenChange"
  >
    <template #content><TaskRefCard v-if="cardProps" v-bind="cardProps" /></template>
  </HoverCard>
</template>

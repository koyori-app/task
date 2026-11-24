<script setup lang="ts">
import { HoverCard } from '@koyori-app/ui-vue';
import { computed, ref } from 'vue';
import TaskRefCard from '@/components/tasks/TaskRefCard.vue';
import { useTaskRefCard } from '@/composables/useTaskRefCard';
import type { TenantUuid } from '@/lib/api-ids';
import {
  TASK_REF_CARD_SELECTOR,
  taskRefCardProps,
  taskRefCardTarget,
  type TaskRefCardTarget,
} from '@/lib/task-ref-card';
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
  // 取得できたら、一覧の行のカードと同じ形の中身にする
  if (task) return taskRefCardProps(task, card.statuses.value, target.value);
  return { ...target.value, state: card.state.value };
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

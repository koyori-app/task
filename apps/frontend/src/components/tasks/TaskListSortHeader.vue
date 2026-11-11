<script setup lang="ts">
import { computed } from 'vue';
import { PhArrowDown, PhArrowUp } from '@phosphor-icons/vue';
import { Dropdown, type DropdownSection } from '@koyori-app/ui-vue';

import {
  activeTaskListSort,
  type TaskListSortColumnOption,
  type TaskListSortingState,
} from '@/components/tasks/task-list-sort';

const props = defineProps<{
  column: TaskListSortColumnOption;
  sorting: TaskListSortingState;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  'update:sorting': [sorting: TaskListSortingState];
}>();

const activeSort = computed(() => activeTaskListSort(props.sorting));
const direction = computed<'asc' | 'desc' | null>(() => {
  if (activeSort.value?.id !== props.column.id) return null;
  return activeSort.value.desc ? 'desc' : 'asc';
});
const triggerLabel = computed(() => {
  const current =
    direction.value === 'asc'
      ? props.column.ascendingLabel
      : direction.value === 'desc'
        ? props.column.descendingLabel
        : null;
  return current
    ? `${props.column.label}を並べ替え、現在は${current}`
    : `${props.column.label}を並べ替え`;
});

const sections = computed<DropdownSection[]>(() => [
  {
    items: [
      {
        value: 'asc',
        label: props.column.ascendingLabel,
        kind: 'radio',
        checked: direction.value === 'asc',
      },
      {
        value: 'desc',
        label: props.column.descendingLabel,
        kind: 'radio',
        checked: direction.value === 'desc',
      },
    ],
  },
  { items: [{ value: 'clear', label: '並べ替えをクリア', disabled: !direction.value }] },
]);

function setDirection(value: 'asc' | 'desc') {
  emit('update:sorting', [{ id: props.column.id, desc: value === 'desc' }]);
}

function onSelect(value: string) {
  if (value === 'asc' || value === 'desc') setDirection(value);
  else if (value === 'clear') clearSorting();
}

function clearSorting() {
  if (direction.value) emit('update:sorting', []);
}

function reverseDirection() {
  if (!direction.value) return;
  setDirection(direction.value === 'asc' ? 'desc' : 'asc');
}
</script>

<template>
  <div class="flex h-8 min-w-0 items-center">
    <!-- 並びが有効なときは右の矢印が向きを示すので、▾ は出さない -->
    <Dropdown
      class="min-w-0 flex-1"
      top-layer
      :label="triggerLabel"
      :disabled="disabled"
      :icon="direction ? null : undefined"
      :items="sections"
      :on-select="onSelect"
    >
      <template #trigger>
        <span class="text-xs font-medium whitespace-nowrap text-muted-foreground">{{
          column.label
        }}</span>
      </template>
    </Dropdown>

    <button
      v-if="direction"
      type="button"
      class="mr-1 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 active:bg-muted/90 disabled:cursor-not-allowed disabled:opacity-50"
      :aria-label="`${column.label}を${direction === 'asc' ? column.descendingLabel : column.ascendingLabel}に反転`"
      :disabled="disabled"
      @click="reverseDirection"
    >
      <PhArrowUp v-if="direction === 'asc'" class="size-3.5" aria-hidden="true" />
      <PhArrowDown v-else class="size-3.5" aria-hidden="true" />
    </button>
  </div>
</template>

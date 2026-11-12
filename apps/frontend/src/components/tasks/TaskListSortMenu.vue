<script setup lang="ts">
import { computed } from 'vue';
import { Dropdown, type DropdownSection } from '@koyori-app/ui-vue';

import {
  activeTaskListSort,
  TASK_LIST_SORT_COLUMNS,
  type TaskListSortColumn,
  type TaskListSortingState,
} from '@/components/tasks/task-list-sort';

/**
 * List 表示の並べ替えを 1 つのメニューにまとめたもの。
 *
 * 列を並べない狭い幅では列の見出し（TaskListSortHeader）を出さないので、代わりにこれを使う。
 */
const props = defineProps<{
  sorting: TaskListSortingState;
}>();

const emit = defineEmits<{
  'update:sorting': [sorting: TaskListSortingState];
}>();

const active = computed(() => activeTaskListSort(props.sorting));

const sections = computed<DropdownSection[]>(() => [
  ...TASK_LIST_SORT_COLUMNS.map((column) => ({
    label: column.label,
    items: [
      {
        value: `${column.id}:asc`,
        label: column.ascendingLabel,
        kind: 'radio' as const,
        checked: active.value?.id === column.id && !active.value.desc,
      },
      {
        value: `${column.id}:desc`,
        label: column.descendingLabel,
        kind: 'radio' as const,
        checked: active.value?.id === column.id && active.value.desc,
      },
    ],
  })),
  { items: [{ value: 'clear', label: '並べ替えをクリア', disabled: !active.value }] },
]);

function onSelect(value: string) {
  if (value === 'clear') {
    emit('update:sorting', []);
    return;
  }
  const [id, direction] = value.split(':') as [TaskListSortColumn, 'asc' | 'desc'];
  emit('update:sorting', [{ id, desc: direction === 'desc' }]);
}
</script>

<template>
  <Dropdown
    variant="tertiary"
    top-layer
    :label="active ? '並び替え（変更あり）' : '並び替え'"
    :items="sections"
    :on-select="onSelect"
  >
    <template #trigger>並び替え</template>
  </Dropdown>
</template>

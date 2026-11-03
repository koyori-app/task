<script setup lang="ts">
import { Dropdown } from '@koyori-app/ui-vue';
import { computed } from 'vue';

import type { ActivityFilter } from '@/lib/activity-timeline';

const OPTIONS: { value: ActivityFilter; label: string }[] = [
  { value: 'all', label: 'すべて' },
  { value: 'comments', label: 'コメント' },
  { value: 'history', label: '履歴' },
];

const filter = defineModel<ActivityFilter>({ default: 'all' });

const current = computed(() => OPTIONS.find((option) => option.value === filter.value)!.label);
const items = computed(() =>
  OPTIONS.map((option) => ({
    ...option,
    kind: 'radio' as const,
    checked: option.value === filter.value,
  })),
);
</script>

<template>
  <Dropdown
    top-layer
    variant="ghost"
    :label="current"
    :ariaLabel="`アクティビティの絞り込み: ${current}`"
    :items="items"
    :on-select="(value: string) => (filter = value as ActivityFilter)"
  />
</template>

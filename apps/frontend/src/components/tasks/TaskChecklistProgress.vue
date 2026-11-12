<script setup lang="ts">
import { computed } from 'vue';

// 説明文のタスクリストの達成率（完了数 / 総数）。表示だけで、押しても何も起きない
const props = defineProps<{ done: number; total: number }>();

const RADIUS = 6;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

const dashOffset = computed(() => CIRCUMFERENCE * (1 - props.done / props.total));
const label = computed(() => `チェックリスト ${props.total} 件中 ${props.done} 件完了`);
</script>

<template>
  <span
    class="inline-flex shrink-0 items-center gap-1 text-xs whitespace-nowrap text-muted-foreground tabular-nums"
    data-checklist-progress
    :title="label"
  >
    <svg class="size-3.5" viewBox="0 0 16 16" aria-hidden="true">
      <circle
        cx="8"
        cy="8"
        :r="RADIUS"
        fill="none"
        stroke="currentColor"
        stroke-opacity="0.25"
        stroke-width="2"
      />
      <!-- 0 件完了では描かない。丸い端が点として残る -->
      <circle
        v-if="done > 0"
        cx="8"
        cy="8"
        :r="RADIUS"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        :stroke-dasharray="CIRCUMFERENCE"
        :stroke-dashoffset="dashOffset"
        transform="rotate(-90 8 8)"
      />
    </svg>
    <span aria-hidden="true">{{ done }} / {{ total }}</span>
    <span class="sr-only">{{ label }}</span>
  </span>
</template>

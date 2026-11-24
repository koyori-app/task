<script setup lang="ts">
import { computed } from 'vue';

import type { StatusFill } from '@/lib/task-status-icon';

/**
 * ステータスのアイコン。「円の埋まり方」で進み具合を表す（点線の円 → 扇形 → 塗りの円にチェック）。
 * 色はステータスに設定された色。名前は隣の文字や読み上げ名が持つので、これ自体は装飾。
 */
const props = withDefaults(
  defineProps<{
    status: { color: string } | null | undefined;
    fill: StatusFill;
    size?: number;
  }>(),
  { size: 16 },
);

const color = computed(() =>
  props.fill.kind === 'unknown' || !props.status?.color
    ? 'var(--koyori-color-text-muted)'
    : props.status.color,
);

/** 12 時から時計回りに fraction だけ埋めた扇形（半径 4） */
const sector = computed(() => {
  const fraction = Math.min(Math.max(props.fill.fraction, 0), 0.999);
  const angle = fraction * 2 * Math.PI;
  const x = 8 + 4 * Math.sin(angle);
  const y = 8 - 4 * Math.cos(angle);
  return `M8 8 L8 4 A4 4 0 ${fraction > 0.5 ? 1 : 0} 1 ${x.toFixed(3)} ${y.toFixed(3)} Z`;
});
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 16 16"
    fill="none"
    class="shrink-0"
    aria-hidden="true"
    data-task-status-icon
    :data-fill="fill.kind"
  >
    <template v-if="fill.kind === 'done'">
      <circle cx="8" cy="8" r="7" :fill="color" />
      <path
        d="M5 8.2 7.1 10.3 11 6"
        stroke="white"
        stroke-width="1.6"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>
    <template v-else-if="fill.kind === 'partial'">
      <circle cx="8" cy="8" r="6.25" :stroke="color" stroke-width="1.5" />
      <path :d="sector" :fill="color" />
    </template>
    <circle
      v-else
      cx="8"
      cy="8"
      r="6.25"
      :stroke="color"
      stroke-width="1.5"
      stroke-dasharray="2.2 2.2"
    />
  </svg>
</template>

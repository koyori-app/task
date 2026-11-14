<script setup lang="ts">
import { computed } from 'vue';

// 説明文のタスクリストの達成率（完了数 / 総数）。表示だけで、押しても何も起きない。
// 根を role="img" にして読み上げ文を aria-label に一本化する（中の文字と輪は読み上げない）
const props = defineProps<{ done: number; total: number }>();

// 輪の外径を描画域いっぱい（半径 + 線幅 / 2 = 8）にして、輪の上下を描画域の上下に合わせる
const RADIUS = 6.5;
const STROKE_WIDTH = 3;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

const dashOffset = computed(() => CIRCUMFERENCE * (1 - props.done / props.total));
const label = computed(() => `チェックリスト ${props.total} 件中 ${props.done} 件完了`);
</script>

<template>
  <span
    class="inline-flex shrink-0 items-baseline gap-1 text-xs whitespace-nowrap text-muted-foreground tabular-nums"
    data-checklist-progress
    role="img"
    :aria-label="label"
    :title="label"
  >
    <!--
      輪は数字と同じ高さ（1cap = その書体の大文字・数字の高さ）にし、下端をベースラインに置く
      （親の items-baseline）。書体や OS が変わっても輪の上下が数字の上下と揃う。
      cap を解さないブラウザーでは size-2（8px）で描く
    -->
    <svg
      class="size-2 overflow-visible supports-[height:1cap]:size-[1cap]"
      viewBox="0 0 16 16"
      aria-hidden="true"
    >
      <circle
        cx="8"
        cy="8"
        :r="RADIUS"
        fill="none"
        stroke="currentColor"
        stroke-opacity="0.25"
        :stroke-width="STROKE_WIDTH"
      />
      <!-- 0 件完了では描かない。丸い端が点として残る -->
      <circle
        v-if="done > 0"
        cx="8"
        cy="8"
        :r="RADIUS"
        fill="none"
        stroke="currentColor"
        :stroke-width="STROKE_WIDTH"
        stroke-linecap="round"
        :stroke-dasharray="CIRCUMFERENCE"
        :stroke-dashoffset="dashOffset"
        transform="rotate(-90 8 8)"
      />
    </svg>
    <span>{{ done }} / {{ total }}</span>
  </span>
</template>

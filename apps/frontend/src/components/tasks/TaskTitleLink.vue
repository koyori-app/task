<script setup lang="ts">
import { computed } from 'vue';

import { taskDetailHref, taskSeqKey } from '@/lib/task-display';

const props = withDefaults(
  defineProps<{
    tenantDisplayId: string;
    projectKey: string;
    seqId: number;
    title: string;
    /** 何行まで出すか。既定は 1 行で省略する */
    lines?: 1 | 2 | 'all';
  }>(),
  { lines: 1 },
);

// inline のままだと truncate が効かず、列の外へはみ出して隣の列に重なる。
// line-clamp は display を自前で持ち、block と並べると block が勝って行数が止まらない
const lineClass = computed(() => {
  if (props.lines === 'all') return 'block break-words';
  if (props.lines === 2) return 'line-clamp-2 break-words';
  return 'block truncate';
});

const emit = defineEmits<{
  select: [seqId: number];
}>();

/**
 * 素の左クリックは呼び出し側に委ね、修飾キー付きは `href`（フルページ）に任せる。
 *
 * 「分割ビューに出すか詳細ページへ送るか」をここで決めない。その判定は画面幅に
 * 依存し、**描画時ではなくクリック時**に読む必要があるため呼び出し側に置く
 * （`select` を受けた側が決める）。真偽値の prop として受け取ると、描画時の値が
 * 固まって古いままになる。
 */
function onPlainClick(event: MouseEvent) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  // vike は document の click でリンクを横取りし defaultPrevented を見ないので、伝播も止める
  event.preventDefault();
  event.stopPropagation();
  emit('select', props.seqId);
}
</script>

<template>
  <a
    :href="taskDetailHref(tenantDisplayId, projectKey, seqId)"
    :data-task-seq-key="taskSeqKey(projectKey, seqId)"
    class="text-sm text-link hover:underline"
    :class="lineClass"
    @click="onPlainClick"
  >
    <!--
      以前はここで `after:absolute after:inset-0` を敷き、当たり判定を行全体へ広げていた。
      `<tr>` は WebKit で絶対配置の包含ブロックにならないため、判定が行を飛び越えて
      テーブル全体へ広がり、「どこをタップしても一番下の行が開く」状態になっていた。
      一覧の行（DataListRow）はクリック操作を持たないので、開くのはこのリンクだけ。
    -->
    {{ title }}
  </a>
</template>

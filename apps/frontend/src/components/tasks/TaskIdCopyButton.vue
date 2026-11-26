<script setup lang="ts">
import { computed } from 'vue';
import { Check, Copy, X } from '@lucide/vue';

import { copyTaskIdLabel, useCopyTaskId } from '@/composables/useCopyTaskId';
import { taskSeqKey } from '@/lib/task-display';

const props = defineProps<{
  projectKey: string;
  seqId: number;
  /**
   * 狭い場所向け（狭い幅の一覧の 2 段目）。番号は縮めず、入らないときはキーの側だけを省略する。
   * アイコンは出さず、押した結果は文字の色と読み上げの名前で伝える。押しても幅が変わらないので、
   * 隣の要素が押し出されない。親より広がらない
   */
  compact?: boolean;
}>();

const seqKey = computed(() => taskSeqKey(props.projectKey, props.seqId));
const { copyState, copyTaskId } = useCopyTaskId();

// 等幅なので ch で揃う。番号は 5 桁まで見込むと（区切りと合わせて + 6）、同じ一覧の行で
// ID の幅が変わらず、後ろに並ぶラベルの開始位置が行をまたいで揃う
const textWidth = computed(() => `${props.projectKey.length + 6}ch`);
// @mousedown.prevent: Safari はボタンを押してもフォーカスせず、外側の行へ移す。行はフォーカスで
// 選択されるので、止めないとコピーのたびに行まで選ばれる。Tab でのフォーカスは残る
</script>

<template>
  <button
    type="button"
    class="inline-flex h-6 shrink-0 items-center rounded-md pr-1 font-mono text-xs transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
    :class="[
      // compact は親の overflow-clip に切られないよう、左へはみ出さず輪も内側に描く
      compact ? 'max-w-full focus-visible:ring-inset' : '-ml-1.5 gap-1 pl-1.5',
      copyState === 'copied'
        ? 'text-(--koyori-color-success)'
        : copyState === 'error'
          ? 'text-destructive'
          : 'text-muted-foreground hover:text-foreground',
    ]"
    :aria-label="copyTaskIdLabel(copyState, seqKey)"
    :title="compact ? seqKey : undefined"
    data-task-id-copy
    @mousedown.prevent
    @click="copyTaskId(seqKey)"
  >
    <template v-if="compact">
      <span class="min-w-0 truncate" data-task-id-key>{{ projectKey }}</span>
      <span class="shrink-0" data-task-id-number>-{{ seqId }}</span>
    </template>
    <template v-else>
      <!-- アイコンの場所は先に確保して、出たときに文字を動かさない -->
      <span class="text-left" :style="{ width: textWidth }">{{ seqKey }}</span>
      <Check v-if="copyState === 'copied'" class="size-3.5 shrink-0" aria-hidden="true" />
      <X v-else-if="copyState === 'error'" class="size-3.5 shrink-0" aria-hidden="true" />
      <Copy
        v-else
        class="size-3.5 shrink-0 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"
        aria-hidden="true"
      />
    </template>
  </button>
</template>

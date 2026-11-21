<script setup lang="ts">
import { computed } from 'vue';
import { Check, Copy, X } from '@lucide/vue';

import { copyTaskIdLabel, useCopyTaskId } from '@/composables/useCopyTaskId';
import { taskSeqKey } from '@/lib/task-display';

const props = defineProps<{
  projectKey: string;
  seqId: number;
  /**
   * 狭い場所向け。番号の桁とアイコンの幅を先に取らず、親より広がらない（あふれる分は省略する）。
   * 隠れたコピーのアイコンは出さず、押した結果（チェック・×）だけを出す
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
    class="-ml-1.5 inline-flex h-6 items-center gap-1 rounded-md pr-1 pl-1.5 font-mono text-xs transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
    :class="[
      compact ? 'max-w-full min-w-0' : 'shrink-0',
      copyState === 'copied'
        ? 'text-(--koyori-color-success)'
        : copyState === 'error'
          ? 'text-destructive'
          : 'text-muted-foreground hover:text-foreground',
    ]"
    :aria-label="copyTaskIdLabel(copyState, seqKey)"
    data-task-id-copy
    @mousedown.prevent
    @click="copyTaskId(seqKey)"
  >
    <!-- アイコンの場所は先に確保して、出たときに文字を動かさない（compact は確保しない） -->
    <span v-if="compact" class="min-w-0 truncate text-left">{{ seqKey }}</span>
    <span v-else class="text-left" :style="{ width: textWidth }">{{ seqKey }}</span>
    <Check v-if="copyState === 'copied'" class="size-3.5 shrink-0" aria-hidden="true" />
    <X v-else-if="copyState === 'error'" class="size-3.5 shrink-0" aria-hidden="true" />
    <Copy
      v-else-if="!compact"
      class="size-3.5 shrink-0 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"
      aria-hidden="true"
    />
  </button>
</template>

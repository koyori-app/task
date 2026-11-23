<script setup lang="ts">
import { computed } from 'vue';

import TaskIdCopyButton from '@/components/tasks/TaskIdCopyButton.vue';
import TaskTitleLink from '@/components/tasks/TaskTitleLink.vue';
import TaskMatchSnippets from '@/components/tasks/query/TaskMatchSnippets.vue';
import type { components } from '@/generated/api';
import { PRIORITY_CONFIG, type ApiPriority } from '@/lib/task-display';

const props = defineProps<{
  tenantDisplayId: string;
  projectKey: string;
  seqId: number;
  title: string;
  priority: ApiPriority;
  /** 768px 以下の形（ID をタイトルの下へ移し、セル全体を的にする）。幅の判定は呼ぶ側 */
  narrow: boolean;
  segments?: components['schemas']['MatchSegment'][];
  snippets?: components['schemas']['MatchSnippet'][];
}>();

const emit = defineEmits<{ select: [seqId: number] }>();

const pc = computed(() => PRIORITY_CONFIG[props.priority]);

// 狭い形は ID の列を畳み、タイトルの下へ移す。タイトルのリンクの after を根の relative な div
// いっぱいに広げ、セルの余白も詳細を開く（TASK-524）。<tr> / <td> を包含ブロックにしない
// （WebKit で表全体へ広がる）。ID はコピーのボタンにして的の上に置く
</script>

<template>
  <div v-if="narrow" class="relative min-w-0 py-1">
    <TaskTitleLink
      :tenant-display-id="tenantDisplayId"
      :project-key="projectKey"
      :seq-id="seqId"
      :title="title"
      :segments="segments"
      :lines="2"
      stretch
      @select="emit('select', $event)"
    />
    <TaskMatchSnippets :snippets="snippets ?? []" />
    <div class="mt-0.5 flex min-w-0 items-center gap-1.5">
      <TaskIdCopyButton class="relative z-10" compact :project-key="projectKey" :seq-id="seqId" />
      <component :is="pc.icon" class="size-4 shrink-0" :style="{ color: pc.color }" />
    </div>
  </div>
  <div v-else class="min-w-0">
    <div class="flex items-center gap-2 min-w-0">
      <component :is="pc.icon" class="size-4 shrink-0" :style="{ color: pc.color }" />
      <TaskTitleLink
        :tenant-display-id="tenantDisplayId"
        :project-key="projectKey"
        :seq-id="seqId"
        :title="title"
        :segments="segments"
        :lines="1"
        @select="emit('select', $event)"
      />
    </div>
    <TaskMatchSnippets :snippets="snippets ?? []" />
  </div>
</template>

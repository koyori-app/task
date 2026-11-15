<script setup lang="ts">
import type { Candidate } from '@/lib/task-query/suggest';
defineProps<{ id: string; candidates: Candidate[]; active: number; heading: string }>();
defineEmits<{ select: [candidate: Candidate] }>();
</script>
<template>
  <div class="rounded border bg-background p-2 shadow-lg">
    <p class="px-2 py-1 text-xs text-muted-foreground">{{ heading }}</p>
    <div :id="id" role="listbox">
      <div
        v-for="(candidate, index) in candidates"
        :id="`${id}-${index}`"
        :key="`${candidate.key ?? ''}:${candidate.value}`"
        role="option"
        :aria-selected="active === index"
        :class="[
          'flex cursor-pointer justify-between gap-2 rounded px-2 py-1.5 text-sm',
          active === index ? 'bg-muted' : '',
        ]"
        @mousedown.prevent
        @click="$emit('select', candidate)"
      >
        <span
          >{{ candidate.label }}
          <span class="text-xs text-muted-foreground">{{ candidate.hint }}</span></span
        ><span v-if="candidate.count !== undefined" class="text-xs text-muted-foreground"
          >{{ candidate.count }} 件</span
        >
      </div>
    </div>
    <p class="mt-2 text-xs text-muted-foreground">
      ↑↓ で選ぶ　Enter で入れる　Esc で閉じる<br />「-」を前に付けると除外（-label:bug）
    </p>
  </div>
</template>

<script setup lang="ts">
import type { components } from '@/generated/api';
import TaskMatchText from './TaskMatchText.vue';
defineProps<{ snippets: components['schemas']['MatchSnippet'][] }>();
function date(value: string) {
  const d = new Date(value);
  return `${d.getMonth() + 1}/${d.getDate()}`;
}
</script>
<template>
  <div
    v-for="(snippet, index) in snippets"
    :key="index"
    class="flex min-w-0 items-center gap-2 text-xs text-muted-foreground"
  >
    <span class="shrink-0 rounded border px-1.5 text-[11px]">{{
      snippet.source === 'comment' ? 'コメント' : '説明'
    }}</span
    ><span v-if="snippet.comment" class="shrink-0"
      >{{ snippet.comment.author?.username ?? '不明' }}・{{
        date(snippet.comment.created_at)
      }}</span
    ><span class="truncate"
      ><template v-if="snippet.truncated_start">…</template
      ><TaskMatchText :segments="snippet.segments" /><template v-if="snippet.truncated_end"
        >…</template
      ></span
    >
  </div>
</template>

<script setup lang="ts">
import { CircleCheck, CircleDashed, CircleDot } from '@lucide/vue';
import { computed } from 'vue';

import { useNow } from '@/composables/useNow';
import type { components } from '@/generated/api';
import { httpUrl, relativeTime } from '@/lib/task-activity';

/** リポジトリセクション先頭の「取り込み元」行。タスクの元になった GitHub Issue を出す */
const props = defineProps<{ issue: components['schemas']['TaskForgeIssue'] }>();

const ISSUE_STATES = {
  open: { icon: CircleDot, label: 'Open', class: 'text-green-700 dark:text-green-400' },
  closed: { icon: CircleCheck, label: 'Closed', class: 'text-violet-700 dark:text-violet-400' },
} as const;
// 同期で状態を控える前に取り込んだ行
const UNKNOWN_STATE = { icon: CircleDashed, label: '状態不明', class: 'text-muted-foreground' };

const state = computed(
  () => ISSUE_STATES[props.issue.state as keyof typeof ISSUE_STATES] ?? UNKNOWN_STATE,
);
const reference = computed(
  () => `${props.issue.repo_owner}/${props.issue.repo_name}#${props.issue.number}`,
);
const now = useNow();
</script>

<template>
  <div class="flex flex-wrap items-center gap-2 text-sm" data-forge-issue-source>
    <span class="shrink-0 text-xs font-medium text-muted-foreground">取り込み元</span>
    <component :is="state.icon" class="size-4 shrink-0" :class="state.class" aria-hidden="true" />
    <a
      v-if="httpUrl(issue.html_url)"
      :href="httpUrl(issue.html_url)!"
      target="_blank"
      rel="noopener noreferrer"
      class="min-w-0 truncate hover:underline"
      >{{ reference }}</a
    >
    <span v-else class="min-w-0 truncate">{{ reference }}</span>
    <span class="shrink-0 rounded border px-1 text-xs" :class="state.class">{{ state.label }}</span>
    <span
      v-if="issue.pending_push"
      class="shrink-0 rounded border px-1 text-xs text-amber-700 dark:text-amber-400"
      >GitHub への反映待ち</span
    >
    <span class="shrink-0 text-xs text-muted-foreground"
      >最終同期 {{ relativeTime(issue.last_synced_at, now) }}</span
    >
  </div>
</template>

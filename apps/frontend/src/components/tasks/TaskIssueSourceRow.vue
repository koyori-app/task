<script setup lang="ts">
import { CircleCheck, CircleDashed, CircleDot } from '@lucide/vue';
import { HoverCard } from '@koyori-app/ui-vue';
import { computed } from 'vue';

import ForgeIssueCard from '@/components/tasks/ForgeIssueCard.vue';
import { useNow } from '@/composables/useNow';
import type { components } from '@/generated/api';
import { httpUrl, relativeTime } from '@/lib/task-activity';

/**
 * リポジトリ欄のタブの行の右端に置く「取り込み元」。タスクの元になった Issue の
 * 状態のアイコン・`owner/repo#番号` のリンク・最終同期を 1 行で出す。状態は色とアイコンで出し、
 * 名前は読み上げにだけ渡す。
 */
const props = defineProps<{
  issue: components['schemas']['TaskForgeIssue'];
  /** ホバーカードに出すタスクの題 */
  title?: string | null;
}>();

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
  <div class="flex min-w-0 items-center gap-2 text-xs" data-forge-issue-source>
    <span class="sr-only">取り込み元</span>
    <HoverCard :label="`${reference} の Issue`">
      <span class="flex min-w-0 items-center gap-1.5">
        <component
          :is="state.icon"
          class="size-4 shrink-0"
          :class="state.class"
          aria-hidden="true"
        />
        <span class="sr-only">{{ state.label }}</span>
        <a
          v-if="httpUrl(issue.html_url)"
          :href="httpUrl(issue.html_url)!"
          target="_blank"
          rel="noopener noreferrer"
          class="min-w-0 truncate hover:underline"
          >{{ reference }}</a
        >
        <span v-else class="min-w-0 truncate">{{ reference }}</span>
      </span>
      <template #content>
        <ForgeIssueCard :issue="issue" :state="state" :title="title" />
      </template>
    </HoverCard>
    <!-- 反映待ちは利用者の気づきを待つ状態なので、ホバーしなくても見える所に警告色で出す -->
    <span v-if="issue.pending_push" class="shrink-0 text-[var(--koyori-color-warning)]"
      >GitHub への反映待ち</span
    >
    <!-- 同期の時刻を控える前の行は null。版の時刻で埋めると同期していないのに新しく見える -->
    <span v-else-if="issue.last_synced_at" class="shrink-0 text-muted-foreground"
      >最終同期 {{ relativeTime(issue.last_synced_at, now) }}</span
    >
  </div>
</template>

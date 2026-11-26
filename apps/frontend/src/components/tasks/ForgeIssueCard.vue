<script setup lang="ts">
import { useNow } from '@/composables/useNow';
import type { components } from '@/generated/api';
import { relativeTime } from '@/lib/task-activity';

/**
 * 取り込み元の Issue のホバーカード（表示だけ）。`owner/repo#番号`・題・最終同期。
 * Issue の題はタスクの題と同じものを同期しているので、タスクの題を出す。作者と作成日は保存していない。
 */
defineProps<{
  issue: components['schemas']['TaskForgeIssue'];
  /** 状態のアイコンと読み上げ名（取り込み元の行と同じもの） */
  state: { icon: unknown; label: string; class: string };
  /** タスクの題 */
  title?: string | null;
}>();

const now = useNow();
</script>

<template>
  <div class="forge-card" data-detail-hover-card data-forge-issue-card>
    <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
      <component :is="state.icon" class="size-4 shrink-0" :class="state.class" aria-hidden="true" />
      <span class="sr-only">{{ state.label }}</span>
      <span class="min-w-0 break-all"
        >{{ issue.repo_owner }}/{{ issue.repo_name }}#{{ issue.number }}</span
      >
    </div>
    <p v-if="title" class="text-sm font-semibold break-words">{{ title }}</p>
    <p v-if="issue.pending_push" class="text-xs text-[var(--koyori-color-warning)]">
      GitHub への反映待ち
    </p>
    <!-- 同期の時刻を控える前の行は null。版の時刻で埋めると同期していないのに新しく見える -->
    <p v-if="issue.last_synced_at" class="text-xs text-muted-foreground">
      最終同期 {{ relativeTime(issue.last_synced_at, now) }}
    </p>
  </div>
</template>

<style scoped>
.forge-card {
  box-sizing: border-box;
  display: grid;
  gap: 8px;
  padding: 12px 14px;
  border-radius: var(--koyori-radius-md, 6px);
}
</style>

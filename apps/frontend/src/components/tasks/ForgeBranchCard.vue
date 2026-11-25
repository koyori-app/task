<script setup lang="ts">
import { GitBranch } from '@lucide/vue';

import { useNow } from '@/composables/useNow';
import type { components } from '@/generated/api';
import { pullRequestState } from '@/lib/forge-pull-request';
import { relativeTime } from '@/lib/task-activity';

/**
 * ブランチのホバーカード（表示だけ）。名前・リンクした時刻・このブランチを使っている PR。
 * 最新のコミットと作者は保存していないので出さない。
 */
defineProps<{ branch: components['schemas']['TaskForgeBranch'] }>();

const now = useNow();
</script>

<template>
  <div class="forge-card" data-detail-hover-card data-forge-branch-card>
    <p class="flex items-center gap-1.5 text-sm font-semibold">
      <GitBranch class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
      <span class="min-w-0 font-mono break-all">{{ branch.branch }}</span>
    </p>
    <p class="text-xs text-muted-foreground">{{ relativeTime(branch.created_at, now) }}にリンク</p>
    <div
      v-if="branch.pull_requests.length"
      class="flex flex-wrap items-center gap-x-2 gap-y-1 border-t pt-2 text-xs text-muted-foreground"
    >
      <span>使っている PR</span>
      <span
        v-for="pullRequest in branch.pull_requests"
        :key="pullRequest.number"
        class="inline-flex items-center gap-1"
      >
        <component
          :is="pullRequestState(pullRequest.state).icon"
          class="size-3.5"
          :class="pullRequestState(pullRequest.state).class"
          aria-hidden="true"
        />
        <span class="sr-only">{{ pullRequestState(pullRequest.state).label }}</span>
        <span class="font-mono">#{{ pullRequest.number }}</span>
      </span>
    </div>
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

<script setup lang="ts">
import { Avatar } from '@koyori-app/ui-vue';

import { useNow } from '@/composables/useNow';
import type { components } from '@/generated/api';
import { pullRequestState } from '@/lib/forge-pull-request';
import { relativeTime } from '@/lib/task-activity';

/** コミットのホバーカード（表示だけ）。本文は 2 行で切り、含まれる PR があれば区切りの下に出す */
defineProps<{ commit: components['schemas']['TaskForgeCommit'] }>();

const now = useNow();
</script>

<template>
  <div class="forge-card" data-detail-hover-card data-forge-commit-card>
    <span class="font-mono text-xs text-muted-foreground">{{ commit.sha.slice(0, 7) }}</span>
    <p class="text-sm font-semibold break-words">{{ commit.message }}</p>
    <p
      v-if="commit.body"
      class="line-clamp-2 text-xs break-words whitespace-pre-line text-muted-foreground"
      data-forge-commit-body
    >
      {{ commit.body }}
    </p>
    <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
      <Avatar :name="commit.author_handle || commit.author_name" :size="16" />
      <span class="truncate">{{ commit.author_handle || commit.author_name }}</span>
      <span class="shrink-0">・{{ relativeTime(commit.committed_at, now) }}</span>
    </div>
    <div
      v-if="commit.pull_requests.length"
      class="flex flex-wrap items-center gap-x-2 gap-y-1 border-t pt-2 text-xs text-muted-foreground"
    >
      <span>含まれる PR</span>
      <span
        v-for="pullRequest in commit.pull_requests"
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

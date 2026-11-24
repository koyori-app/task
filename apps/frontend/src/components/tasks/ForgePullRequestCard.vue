<script setup lang="ts">
import { GitBranch } from '@lucide/vue';
import { Avatar } from '@koyori-app/ui-vue';

import type { components } from '@/generated/api';
import { mergeEffectsPending } from '@/composables/useTaskForge';
import { pullRequestState } from '@/lib/forge-pull-request';

/**
 * PR のホバーカード（表示だけ。取得しない）。状態はアイコンで出し、名前は読み上げにだけ渡す。
 * レビューのある PR だけ、区切りの下に未解決の件数（レビュー画面へのリンク）を出す。
 */
defineProps<{
  pullRequest: components['schemas']['TaskForgePullRequest'];
  /** レビュー画面の URL。未解決の件数のリンク先 */
  reviewHref: string;
}>();
</script>

<template>
  <div class="forge-card" data-detail-hover-card data-forge-pull-request-card>
    <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
      <component
        :is="pullRequestState(pullRequest.state).icon"
        class="size-4 shrink-0"
        :class="pullRequestState(pullRequest.state).class"
        aria-hidden="true"
      />
      <span class="sr-only">{{ pullRequestState(pullRequest.state).label }}</span>
      <span class="font-mono">#{{ pullRequest.number }}</span>
    </div>
    <p class="text-sm font-semibold break-words">{{ pullRequest.title || '（題なし）' }}</p>
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
      <span v-if="pullRequest.author_handle" class="flex min-w-0 items-center gap-1.5">
        <Avatar :name="pullRequest.author_handle" :size="16" />
        <span class="truncate">{{ pullRequest.author_handle }}</span>
      </span>
      <span v-if="pullRequest.head_branch" class="flex min-w-0 items-center gap-1">
        <GitBranch class="size-3.5 shrink-0" aria-hidden="true" />
        <span class="truncate font-mono">{{ pullRequest.head_branch }}</span>
      </span>
    </div>
    <p
      v-if="mergeEffectsPending(pullRequest)"
      class="text-xs text-[var(--koyori-color-warning)]"
      data-forge-card-merge-effects
    >
      マージ済み（自動完了は未適用）
    </p>
    <div v-if="pullRequest.unresolved != null" class="border-t pt-2 text-xs">
      <a
        :href="reviewHref"
        class="underline underline-offset-2"
        :class="pullRequest.unresolved ? 'text-destructive' : 'text-muted-foreground'"
        data-forge-unresolved
        >{{
          pullRequest.unresolved ? `レビュー ${pullRequest.unresolved} 件が未解決` : '未解決なし'
        }}</a
      >
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

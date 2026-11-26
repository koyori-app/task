<script setup lang="ts">
import { Dropdown, HoverCard, type DropdownItem } from '@koyori-app/ui-vue';
import { computed, ref } from 'vue';

import ForgePullRequestCard from '@/components/tasks/ForgePullRequestCard.vue';
import TaskForgeMergeEffectsBadge from '@/components/tasks/TaskForgeMergeEffectsBadge.vue';
import TaskForgeUnlinkDialog from '@/components/tasks/TaskForgeUnlinkDialog.vue';
import {
  mergeEffectsPending,
  useTaskForge,
  useTaskForgePullRequestActions,
  type TaskForgePullRequest,
} from '@/composables/useTaskForge';
import { pullRequestState } from '@/lib/forge-pull-request';
import { reviewDetailHref } from '@/lib/review-url-state';
import { httpUrl } from '@/lib/task-activity';

/**
 * 題の上に並べる、このタスクを実装する PR のピル（状態のアイコンと番号）。
 *
 * 実装の PR はここにだけ出し、リポジトリ欄の一覧には出さない（TASK-525）。ホバーで PR のカード、
 * 押すとメニュー（ホストで開く・言及に戻す・リンクを外す）。forge の取得はリポジトリ欄と分け合う。
 */
const props = defineProps<{
  tenantId: string | null;
  projectId: string | null;
  taskId: string;
  /** レビュー画面へのリンクに使う */
  tenantDisplayId: string;
  projectKey: string;
}>();

const forgeQuery = useTaskForge({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.taskId,
});
const actions = useTaskForgePullRequestActions({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.taskId,
});

const pullRequests = computed(() =>
  (forgeQuery.data.value?.pull_requests ?? []).filter((pr) => pr.role === 'implementation'),
);

const HOST_NAMES: Record<string, string> = {
  github: 'GitHub',
  gitlab: 'GitLab',
  forgejo: 'Forgejo',
};

function menuItems(pullRequest: TaskForgePullRequest): DropdownItem[] {
  return [
    ...(httpUrl(pullRequest.html_url)
      ? [{ value: 'open', label: `${HOST_NAMES[pullRequest.host] ?? 'ホスト'} で開く` }]
      : []),
    ...(mergeEffectsPending(pullRequest)
      ? [{ value: 'merge-effects', label: '自動完了の未適用を確認' }]
      : []),
    { value: 'mention', label: '言及に戻す', disabled: actions.rolePending.value },
    { value: 'unlink', label: 'リンクを外す', destructive: true },
  ];
}

// 未適用の確認のダイアログは札の部品が持つ。ピルごとに 1 つ置き、メニューから開く
const mergeEffectsDialogs = new Map<string, { openDialog: () => void }>();

function onSelect(pullRequest: TaskForgePullRequest, value: string) {
  if (value === 'open') {
    const url = httpUrl(pullRequest.html_url);
    if (url) window.open(url, '_blank', 'noopener,noreferrer');
  } else if (value === 'merge-effects') {
    mergeEffectsDialogs.get(pullRequest.id)?.openDialog();
  } else if (value === 'mention') {
    void actions.setRole(pullRequest, 'mention');
  } else if (value === 'unlink') {
    actions.requestUnlink(pullRequest);
  }
}

// メニューが開いている間はカードを出さない（重なる）
const openMenuId = ref<string | null>(null);
</script>

<template>
  <span
    v-for="pullRequest in pullRequests"
    :key="pullRequest.id"
    class="inline-flex"
    data-implementation-pull-request
  >
    <HoverCard :label="`#${pullRequest.number} の PR`" :disabled="openMenuId === pullRequest.id">
      <span class="relative inline-flex h-7 items-center rounded-md border">
        <Dropdown
          top-layer
          :label="`#${pullRequest.number} の操作`"
          :icon="null"
          :items="menuItems(pullRequest)"
          :on-select="(value: string) => onSelect(pullRequest, value)"
          :on-open-change="
            (open: boolean) =>
              (openMenuId = open
                ? pullRequest.id
                : openMenuId === pullRequest.id
                  ? null
                  : openMenuId)
          "
        >
          <template #trigger>
            <span class="flex items-center gap-1 text-xs">
              <component
                :is="pullRequestState(pullRequest.state).icon"
                class="size-3.5 shrink-0"
                :class="pullRequestState(pullRequest.state).class"
                aria-hidden="true"
              />
              <span class="sr-only">{{ pullRequestState(pullRequest.state).label }}</span>
              <span class="font-mono">#{{ pullRequest.number }}</span>
            </span>
          </template>
        </Dropdown>
        <!-- 自動完了が未適用（確認待ち）。ホバーしなくても気づけるよう、ピルに点を付ける -->
        <span
          v-if="mergeEffectsPending(pullRequest)"
          class="absolute -top-1 -right-1 size-2 rounded-full bg-[var(--koyori-color-warning)]"
          data-merge-effects-dot
        >
          <span class="sr-only">自動完了は未適用</span>
        </span>
      </span>
      <template #content>
        <ForgePullRequestCard
          :pull-request="pullRequest"
          :review-href="reviewDetailHref(tenantDisplayId, projectKey, pullRequest.number)"
        />
      </template>
    </HoverCard>
    <TaskForgeMergeEffectsBadge
      v-if="tenantId && mergeEffectsPending(pullRequest)"
      :ref="
        (el) =>
          el
            ? mergeEffectsDialogs.set(pullRequest.id, el as unknown as { openDialog: () => void })
            : mergeEffectsDialogs.delete(pullRequest.id)
      "
      trigger="none"
      :tenant-id="tenantId"
      :pull-request="pullRequest"
    />
  </span>
  <p v-if="actions.roleError.value" role="alert" class="basis-full text-xs text-destructive">
    {{ actions.roleError.value }}
  </p>
  <TaskForgeUnlinkDialog
    :target="actions.unlinkTarget.value"
    :pending="actions.unlinkPending.value"
    :error="actions.unlinkError.value"
    @cancel="actions.cancelUnlink"
    @confirm="actions.confirmUnlink"
  />
</template>

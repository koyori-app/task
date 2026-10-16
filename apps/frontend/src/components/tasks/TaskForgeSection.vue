<script setup lang="ts">
import { GitCommitHorizontal } from '@lucide/vue';
import { useQuery } from '@tanstack/vue-query';
import { computed } from 'vue';

import { Button } from '@/components/ui/button';
import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';
import { pullRequestState as prState } from '@/lib/forge-pull-request';
import { httpUrl } from '@/lib/task-activity';

/**
 * タスク詳細の「リポジトリ」セクション。タスクに結ばれた PR とコミットを並べる
 * （docs/features/tasks/9.github-tasks.md §9）。
 *
 * PR をリンクする操作とブランチの表示は後続のタスクで足す。
 */
const FORGE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge' as const;

type PullRequest = components['schemas']['TaskForgePullRequest'];

const props = defineProps<{
  tenantId: string | null;
  projectId: string | null;
  taskId: string;
  /** レビュー画面へのリンクに使う（`/{tenant}/projects/{key}/reviews`） */
  tenantDisplayId: string;
  projectKey: string;
}>();

const forgeQuery = useQuery({
  queryKey: computed(() => [
    'get',
    FORGE_PATH,
    { tenantId: props.tenantId, projectId: props.projectId, taskId: props.taskId },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(FORGE_PATH, {
      params: {
        path: { tenant_id: props.tenantId!, project_id: props.projectId!, id: props.taskId },
      },
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!props.tenantId && !!props.projectId && !!props.taskId),
  retry: false,
});

const pullRequests = computed(() => forgeQuery.data.value?.pull_requests ?? []);
const commits = computed(() => forgeQuery.data.value?.commits ?? []);
// GitLab は Merge Request と呼ぶ
const pullRequestHeading = computed(() =>
  pullRequests.value.length > 0 && pullRequests.value.every((pr) => pr.host === 'gitlab')
    ? 'Merge Requests'
    : 'Pull Requests',
);

// 未解決件数は `read:review` が無い・レビューが無い PR では null（表示しない）
function reviewHref(pr: PullRequest): string {
  return `/${props.tenantDisplayId}/projects/${props.projectKey}/reviews?pr=${pr.number}`;
}

const SOURCE_LABELS: Record<string, string> = {
  title: 'タイトル',
  body: '本文',
  branch: 'ブランチ',
  manual: '手動',
};
</script>

<template>
  <section class="space-y-2" aria-label="リポジトリ" data-task-forge-section>
    <h2 class="text-sm font-semibold">リポジトリ</h2>

    <p v-if="forgeQuery.isLoading.value" class="text-sm text-muted-foreground">読み込み中…</p>
    <div
      v-else-if="forgeQuery.isError.value"
      class="flex items-center gap-2 text-sm text-destructive"
    >
      リポジトリの情報を読み込めませんでした
      <Button type="button" variant="outline" size="sm" @click="forgeQuery.refetch()"
        >再試行</Button
      >
    </div>
    <p v-else-if="!pullRequests.length && !commits.length" class="text-sm text-muted-foreground">
      まだ PR もコミットもありません
    </p>
    <template v-else>
      <div v-if="pullRequests.length" class="space-y-1">
        <h3 class="text-xs font-medium text-muted-foreground">{{ pullRequestHeading }}</h3>
        <ul class="space-y-1">
          <li
            v-for="pr in pullRequests"
            :key="pr.id"
            class="flex items-center gap-2 text-sm"
            data-forge-pull-request
          >
            <component
              :is="prState(pr.state).icon"
              class="size-4 shrink-0"
              :class="prState(pr.state).class"
              aria-hidden="true"
            />
            <span class="sr-only">{{ prState(pr.state).label }}</span>
            <a
              v-if="httpUrl(pr.html_url)"
              :href="httpUrl(pr.html_url)!"
              target="_blank"
              rel="noopener noreferrer"
              class="min-w-0 flex-1 truncate hover:underline"
              :title="pr.title"
            >
              <span class="text-muted-foreground">#{{ pr.number }}</span> {{ pr.title }}
            </a>
            <span v-else class="min-w-0 flex-1 truncate" :title="pr.title">
              <span class="text-muted-foreground">#{{ pr.number }}</span> {{ pr.title }}
            </span>
            <span
              v-for="source in pr.sources"
              :key="source"
              class="shrink-0 rounded border px-1 text-xs text-muted-foreground"
            >
              {{ SOURCE_LABELS[source] ?? source }}
            </span>
            <span v-if="pr.author_handle" class="shrink-0 text-xs text-muted-foreground">
              {{ pr.author_handle }}
            </span>
            <a
              v-if="pr.unresolved != null"
              :href="reviewHref(pr)"
              class="shrink-0 text-xs underline underline-offset-2"
              :class="pr.unresolved ? 'text-destructive' : 'text-muted-foreground'"
              data-forge-unresolved
              >{{ pr.unresolved ? `${pr.unresolved} 件が未解決` : '未解決なし' }}</a
            >
          </li>
        </ul>
      </div>

      <div v-if="commits.length" class="space-y-1">
        <h3 class="text-xs font-medium text-muted-foreground">Commits</h3>
        <ul class="space-y-1">
          <li
            v-for="commit in commits"
            :key="commit.id"
            class="flex items-center gap-2 text-sm"
            data-forge-commit
          >
            <GitCommitHorizontal class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            <a
              v-if="httpUrl(commit.html_url)"
              :href="httpUrl(commit.html_url)!"
              target="_blank"
              rel="noopener noreferrer"
              class="shrink-0 font-mono text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
              >{{ commit.sha.slice(0, 7) }}</a
            >
            <span v-else class="shrink-0 font-mono text-xs text-muted-foreground">{{
              commit.sha.slice(0, 7)
            }}</span>
            <span class="min-w-0 flex-1 truncate" :title="commit.message">{{
              commit.message
            }}</span>
            <span class="shrink-0 text-xs text-muted-foreground">
              {{ commit.author_handle || commit.author_name }}
            </span>
          </li>
        </ul>
      </div>
    </template>
  </section>
</template>

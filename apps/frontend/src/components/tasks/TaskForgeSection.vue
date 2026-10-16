<script setup lang="ts">
import {
  CircleDashed,
  GitCommitHorizontal,
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  MoreHorizontal,
  Plus,
} from '@lucide/vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { computed, ref } from 'vue';

import TaskForgePullRequestLinkDialog from '@/components/tasks/TaskForgePullRequestLinkDialog.vue';
import TaskIssueSourceRow from '@/components/tasks/TaskIssueSourceRow.vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import type { components } from '@/generated/api';
import { apiClient, fetchClient } from '@/lib/api-vue-query';
import { httpUrl } from '@/lib/task-activity';

/**
 * タスク詳細の「リポジトリ」セクション。タスクに結ばれた PR とコミットを並べる
 * （docs/features/tasks/9.github-tasks.md §9）。
 *
 * ブランチの表示は後続のタスクで足す。
 */
const FORGE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge' as const;
const UNLINK_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge/pull-requests/{pull_request_id}' as const;

type PullRequest = components['schemas']['TaskForgePullRequest'];

const props = defineProps<{
  tenantId: string | null;
  projectId: string | null;
  taskId: string;
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

// 連携先は forge と一緒に返る。null なら未連携（読み込み前・取得失敗はボタンを止めない）
const repository = computed(() => forgeQuery.data.value?.repository);
const notConnected = computed(() => repository.value === null);
const repoFullName = computed(() =>
  repository.value ? `${repository.value.repo_owner}/${repository.value.repo_name}` : null,
);
const isLinkOpen = ref(false);

const queryClient = useQueryClient();
const unlinkMutation = apiClient.useMutation('delete', UNLINK_PATH);
const unlinkTarget = ref<PullRequest | null>(null);
const unlinkError = ref<string | null>(null);

function onUnlinkOpenChange(open: boolean) {
  if (!open && unlinkMutation.isPending.value) return;
  if (!open) unlinkTarget.value = null;
}

async function confirmUnlink() {
  const target = unlinkTarget.value;
  if (!target || !props.tenantId || !props.projectId) return;
  unlinkError.value = null;
  try {
    await unlinkMutation.mutateAsync({
      params: {
        path: {
          tenant_id: props.tenantId,
          project_id: props.projectId,
          id: props.taskId,
          pull_request_id: target.id,
        },
      },
    });
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', FORGE_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', ACTIVITIES_PATH] }),
    ]);
    unlinkTarget.value = null;
  } catch {
    unlinkError.value = 'リンクを外せませんでした';
  }
}

const pullRequests = computed(() => forgeQuery.data.value?.pull_requests ?? []);
const commits = computed(() => forgeQuery.data.value?.commits ?? []);
// GitLab は Merge Request と呼ぶ
const pullRequestHeading = computed(() =>
  pullRequests.value.length > 0 && pullRequests.value.every((pr) => pr.host === 'gitlab')
    ? 'Merge Requests'
    : 'Pull Requests',
);

const PR_STATES = {
  open: { icon: GitPullRequest, label: 'Open', class: 'text-green-700 dark:text-green-400' },
  merged: { icon: GitMerge, label: 'Merged', class: 'text-violet-700 dark:text-violet-400' },
  closed: { icon: GitPullRequestClosed, label: 'Closed', class: 'text-destructive' },
} as const;
// webhook をまだ受けていない PR（CLI のレビューだけで作られた行など）
const UNKNOWN_STATE = { icon: CircleDashed, label: '状態不明', class: 'text-muted-foreground' };

function prState(pr: PullRequest) {
  return PR_STATES[pr.state as keyof typeof PR_STATES] ?? UNKNOWN_STATE;
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
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-sm font-semibold">リポジトリ</h2>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        :disabled="notConnected"
        :aria-describedby="notConnected ? 'forge-link-disabled-reason' : undefined"
        @click="isLinkOpen = true"
      >
        <Plus class="size-4" aria-hidden="true" />
        PR をリンク
      </Button>
    </div>
    <p v-if="notConnected" id="forge-link-disabled-reason" class="text-xs text-muted-foreground">
      PR をリンクするには、プロジェクト設定でリポジトリを連携してください
    </p>
    <TaskIssueSourceRow v-if="forgeQuery.data.value?.issue" :issue="forgeQuery.data.value.issue" />

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
              :is="prState(pr).icon"
              class="size-4 shrink-0"
              :class="prState(pr).class"
              aria-hidden="true"
            />
            <span class="sr-only">{{ prState(pr).label }}</span>
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
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  :aria-label="`#${pr.number} の操作`"
                >
                  <MoreHorizontal class="size-4" aria-hidden="true" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  variant="destructive"
                  @select="
                    unlinkError = null;
                    unlinkTarget = pr;
                  "
                >
                  リンクを外す
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
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

    <TaskForgePullRequestLinkDialog
      v-if="isLinkOpen && tenantId && projectId"
      :tenant-id="tenantId"
      :project-id="projectId"
      :task-id="taskId"
      :repo-full-name="repoFullName"
      @close="isLinkOpen = false"
    />
    <Dialog v-if="unlinkTarget" :open="true" @update:open="onUnlinkOpenChange">
      <DialogContent class="max-w-md" :show-close-button="false">
        <DialogHeader>
          <DialogTitle>PR のリンクを外しますか？</DialogTitle>
          <DialogDescription>
            #{{ unlinkTarget.number }} とこのタスクのリンクをすべて外します。PR
            のタイトル・本文にタスクの ID が残っていると、次に PR
            が更新されたときに再びリンクされます。
          </DialogDescription>
        </DialogHeader>
        <p v-if="unlinkError" role="alert" class="text-sm text-destructive">{{ unlinkError }}</p>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            :disabled="unlinkMutation.isPending.value"
            @click="unlinkTarget = null"
          >
            キャンセル
          </Button>
          <Button
            type="button"
            variant="destructive"
            :disabled="unlinkMutation.isPending.value"
            @click="confirmUnlink"
          >
            {{ unlinkMutation.isPending.value ? '外しています…' : 'リンクを外す' }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </section>
</template>

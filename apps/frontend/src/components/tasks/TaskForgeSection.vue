<script setup lang="ts">
import { GitCommitHorizontal, MoreHorizontal, Plus } from '@lucide/vue';
import { computed, ref, watch } from 'vue';

import ForgeCommitCard from '@/components/tasks/ForgeCommitCard.vue';
import ForgePullRequestCard from '@/components/tasks/ForgePullRequestCard.vue';
import TaskForgeMergeEffectsBadge from '@/components/tasks/TaskForgeMergeEffectsBadge.vue';
import TaskForgePullRequestLinkDialog from '@/components/tasks/TaskForgePullRequestLinkDialog.vue';
import TaskForgeUnlinkDialog from '@/components/tasks/TaskForgeUnlinkDialog.vue';
import TaskIssueSourceRow from '@/components/tasks/TaskIssueSourceRow.vue';
import {
  Avatar,
  Button,
  Dropdown,
  HoverCard,
  TabPanel,
  Tabs,
  type DropdownItem,
} from '@koyori-app/ui-vue';
import { useNow } from '@/composables/useNow';
import {
  useTaskForge,
  useTaskForgePullRequestActions,
  type TaskForgePullRequest,
} from '@/composables/useTaskForge';
import { pullRequestState as prState } from '@/lib/forge-pull-request';
import { reviewDetailHref } from '@/lib/review-url-state';
import { httpUrl, relativeTime } from '@/lib/task-activity';

import TaskForgeBranches from './TaskForgeBranches.vue';

/**
 * タスク詳細の「リポジトリ」セクション。タスクに触れているだけの PR・コミット・ブランチを
 * タブで並べる（docs/dev/features/tasks/github-tasks/index.md の「画面」）。
 *
 * このタスクを実装する PR はここに出さず、題の上のピル（TaskImplementationPullRequests）に出す。
 * forge の取得はピルと同じクエリキーで分け合う。
 */
type TabValue = 'pull_requests' | 'commits' | 'branches';

const props = defineProps<{
  tenantId: string | null;
  projectId: string | null;
  taskId: string;
  /** レビュー画面へのリンクに使う（`/{tenant}/projects/{key}/reviews`） */
  tenantDisplayId: string;
  projectKey: string;
  /** 取り込み元の Issue のホバーカードに出すタスクの題 */
  taskTitle?: string | null;
}>();

const target = {
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.taskId,
};
const forgeQuery = useTaskForge(target);
const actions = useTaskForgePullRequestActions(target);

// 連携先は forge と一緒に返る。null なら未連携（読み込み前・取得失敗はボタンを止めない）
const repository = computed(() => forgeQuery.data.value?.repository);
const notConnected = computed(() => repository.value === null);
const repoFullName = computed(() =>
  repository.value ? `${repository.value.repo_owner}/${repository.value.repo_name}` : null,
);
const isLinkOpen = ref(false);

const allPullRequests = computed(() => forgeQuery.data.value?.pull_requests ?? []);
/** 一覧に出すのは言及だけの PR。実装の PR は題の上のピルに出る */
const pullRequests = computed(() => allPullRequests.value.filter((pr) => pr.role === 'mention'));
const commits = computed(() => forgeQuery.data.value?.commits ?? []);
const branches = computed(() => forgeQuery.data.value?.branches ?? []);
// GitLab は Merge Request と呼ぶ
const pullRequestHeading = computed(() =>
  allPullRequests.value.length > 0 && allPullRequests.value.every((pr) => pr.host === 'gitlab')
    ? 'Merge Requests'
    : 'Pull Requests',
);
const nothingLinked = computed(
  () => !allPullRequests.value.length && !commits.value.length && !branches.value.length,
);

const tabItems = computed(() => [
  { value: 'pull_requests', label: `${pullRequestHeading.value} ${pullRequests.value.length}` },
  { value: 'commits', label: `コミット ${commits.value.length}` },
  { value: 'branches', label: `ブランチ ${branches.value.length}` },
]);
/**
 * 既定のタブは、Pull Requests → コミット → ブランチの順で最初に 1 件以上あるもの。
 * 実装の PR を一覧に出さないので、Pull Requests が 0 件のタスクが多い。
 */
const defaultTab = computed<TabValue>(() => {
  if (pullRequests.value.length) return 'pull_requests';
  if (commits.value.length) return 'commits';
  if (branches.value.length) return 'branches';
  return 'pull_requests';
});
// 利用者が選んだ後は、件数が変わっても動かさない
const selectedTab = ref<TabValue | null>(null);
const tab = computed(() => selectedTab.value ?? defaultTab.value);
// オーバーレイと右ペインは別のタスクへ移っても同じインスタンス
watch(
  () => props.taskId,
  () => {
    selectedTab.value = null;
  },
);

const PR_ACTION_ITEMS = computed<DropdownItem[]>(() => [
  { value: 'implementation', label: '実装 PR にする', disabled: actions.rolePending.value },
  { value: 'unlink', label: 'リンクを外す', destructive: true },
]);

function onPullRequestAction(pullRequest: TaskForgePullRequest, value: string) {
  if (value === 'implementation') void actions.setRole(pullRequest, 'implementation');
  else if (value === 'unlink') actions.requestUnlink(pullRequest);
}

function reviewHref(pr: TaskForgePullRequest): string {
  return reviewDetailHref(props.tenantDisplayId, props.projectKey, pr.number);
}

const now = useNow();
</script>

<template>
  <section class="border-t pt-5" aria-label="リポジトリ" data-task-forge-section>
    <div class="mb-1 flex min-h-8 items-center justify-between gap-2">
      <h2 class="text-sm font-semibold">リポジトリ</h2>
      <Button
        type="button"
        variant="tertiary"
        :disabled="notConnected"
        :aria-describedby="notConnected ? 'forge-link-disabled-reason' : undefined"
        label="PR をリンク"
        @click="isLinkOpen = true"
      >
        <template #icon><Plus class="size-4" aria-hidden="true" /></template>
      </Button>
    </div>
    <p
      v-if="notConnected"
      id="forge-link-disabled-reason"
      class="mb-2 text-xs text-muted-foreground"
    >
      PR をリンクするには、プロジェクト設定でリポジトリを連携してください
    </p>

    <p v-if="forgeQuery.isLoading.value" class="text-sm text-muted-foreground">読み込み中…</p>
    <div
      v-else-if="forgeQuery.isError.value"
      class="flex items-center gap-2 text-sm text-destructive"
    >
      リポジトリの情報を読み込めませんでした
      <Button type="button" variant="tertiary" label="再試行" @click="forgeQuery.refetch()" />
    </div>
    <!-- タブの行の右端に取り込み元を置く。狭い幅ではタブの下へ回す -->
    <div v-else class="relative [&_[role=tabpanel]]:px-0 [&_[role=tabpanel]]:py-2">
      <Tabs
        id="task-forge-tabs"
        label="リポジトリ"
        :items="tabItems"
        :value="tab"
        :on-value-change="(value: string) => (selectedTab = value as TabValue)"
      >
        <TaskIssueSourceRow
          v-if="forgeQuery.data.value?.issue"
          class="mt-2 sm:absolute sm:top-0 sm:right-0 sm:mt-0 sm:h-9 sm:max-w-[55%]"
          :issue="forgeQuery.data.value.issue"
          :title="taskTitle"
        />

        <TabPanel value="pull_requests">
          <ul v-if="pullRequests.length" class="divide-y divide-border/60">
            <li
              v-for="pr in pullRequests"
              :key="pr.id"
              class="flex min-h-12 items-center gap-2 py-1.5 text-sm"
              data-forge-pull-request
            >
              <component
                :is="prState(pr.state).icon"
                class="size-4 shrink-0"
                :class="prState(pr.state).class"
                aria-hidden="true"
              />
              <span class="sr-only">{{ prState(pr.state).label }}</span>
              <div class="min-w-0 flex-1">
                <HoverCard :label="`#${pr.number} の PR`">
                  <span class="block min-w-0">
                    <a
                      v-if="httpUrl(pr.html_url)"
                      :href="httpUrl(pr.html_url)!"
                      target="_blank"
                      rel="noopener noreferrer"
                      class="block truncate font-medium hover:underline"
                      >{{ pr.title || `#${pr.number}` }}</a
                    >
                    <span v-else class="block truncate font-medium">{{
                      pr.title || `#${pr.number}`
                    }}</span>
                  </span>
                  <template #content>
                    <ForgePullRequestCard :pull-request="pr" :review-href="reviewHref(pr)" />
                  </template>
                </HoverCard>
                <div class="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                  <span class="shrink-0 font-mono">#{{ pr.number }}</span>
                  <template v-if="pr.author_handle">
                    <span aria-hidden="true">・</span>
                    <Avatar :name="pr.author_handle" :size="14" />
                    <span class="truncate">{{ pr.author_handle }}</span>
                  </template>
                  <!-- 利用者の操作を待つ状態なので、ホバーしなくても見える行の中に出す -->
                  <TaskForgeMergeEffectsBadge
                    v-if="tenantId"
                    :tenant-id="tenantId"
                    :pull-request="pr"
                  />
                </div>
              </div>
              <Dropdown
                top-layer
                variant="ghost"
                class="shrink-0"
                :ariaLabel="`#${pr.number} の操作`"
                :items="PR_ACTION_ITEMS"
                :on-select="(value: string) => onPullRequestAction(pr, value)"
              >
                <template #icon><MoreHorizontal class="size-4" aria-hidden="true" /></template>
              </Dropdown>
            </li>
          </ul>
          <p v-else class="text-sm text-muted-foreground" data-forge-empty>
            {{
              nothingLinked ? 'まだ PR もコミットもありません' : '触れているだけの PR はありません'
            }}
          </p>
          <p v-if="actions.roleError.value" role="alert" class="mt-1 text-sm text-destructive">
            {{ actions.roleError.value }}
          </p>
        </TabPanel>

        <TabPanel value="commits">
          <ul v-if="commits.length" class="divide-y divide-border/60">
            <li
              v-for="commit in commits"
              :key="commit.id"
              class="flex min-h-12 items-center gap-2 py-1.5 text-sm"
              data-forge-commit
            >
              <GitCommitHorizontal
                class="size-4 shrink-0 text-muted-foreground"
                aria-hidden="true"
              />
              <div class="min-w-0 flex-1">
                <HoverCard :label="`${commit.sha.slice(0, 7)} のコミット`">
                  <span class="block truncate font-medium">{{ commit.message }}</span>
                  <template #content><ForgeCommitCard :commit="commit" /></template>
                </HoverCard>
                <div class="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                  <a
                    v-if="httpUrl(commit.html_url)"
                    :href="httpUrl(commit.html_url)!"
                    target="_blank"
                    rel="noopener noreferrer"
                    class="shrink-0 font-mono underline underline-offset-2 hover:text-foreground"
                    >{{ commit.sha.slice(0, 7) }}</a
                  >
                  <span v-else class="shrink-0 font-mono">{{ commit.sha.slice(0, 7) }}</span>
                  <span aria-hidden="true">・</span>
                  <span class="truncate">{{ commit.author_handle || commit.author_name }}</span>
                  <span aria-hidden="true">・</span>
                  <span class="shrink-0">{{ relativeTime(commit.committed_at, now) }}</span>
                </div>
              </div>
            </li>
          </ul>
          <p v-else class="text-sm text-muted-foreground">コミットはありません</p>
        </TabPanel>

        <TabPanel value="branches">
          <TaskForgeBranches
            v-if="branches.length && tenantId && projectId"
            :tenant-id="tenantId"
            :project-id="projectId"
            :task-id="taskId"
            :branches="branches"
          />
          <p v-else class="text-sm text-muted-foreground">ブランチはありません</p>
        </TabPanel>
      </Tabs>
    </div>

    <TaskForgePullRequestLinkDialog
      v-if="isLinkOpen && tenantId && projectId"
      :tenant-id="tenantId"
      :project-id="projectId"
      :task-id="taskId"
      :repo-full-name="repoFullName"
      @close="isLinkOpen = false"
    />
    <TaskForgeUnlinkDialog
      :target="actions.unlinkTarget.value"
      :pending="actions.unlinkPending.value"
      :error="actions.unlinkError.value"
      @cancel="actions.cancelUnlink"
      @confirm="actions.confirmUnlink"
    />
  </section>
</template>

<script setup lang="ts">
import LoadErrorState from '@/components/LoadErrorState.vue';
import { Loader2 } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { keepPreviousData, useQuery } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';

import { Button, Spinner } from '@koyori-app/ui-vue';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';
import {
  MY_TASKS_MAX_SHOWN,
  MY_TASKS_PAGE_LIMIT,
  applyMyTasksUrlState,
  myTasksEmptyMessage,
  parseMyTasksUrlState,
  type MyTasksTab,
} from '@/lib/my-tasks-url-state';
import { taskDetailHref } from '@/lib/task-display';

type MyTaskItem = components['schemas']['MyTaskItem'];

const TASKS_PATH = '/v1/tenants/{tenant_id}/users/me/tasks' as const;

const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
  tenantsQuery,
} = useResolvedTenantId(tenantDisplayId);

const tabs: { key: MyTasksTab; label: string }[] = [
  { key: 'today', label: '今日' },
  { key: 'week', label: '今週' },
  { key: 'no_due_date', label: '期限なし' },
  { key: 'overdue', label: '期限超過' },
  { key: 'all', label: 'すべて' },
];

// タブと読み込んだ件数は URL（`?tab=`・`?shown=`）に持ち、タスクを開いて戻っても同じ画面に戻す
const urlState = computed(() => parseMyTasksUrlState(pageContext.urlParsed?.search));
const activeFilter = ref<MyTasksTab>(urlState.value.tab);
const offset = ref(0);
// 次に取る件数。URL から復元するときは読み込んでいた件数を 1 回で取る
const limit = ref(urlState.value.shown);
const loadedTasks = ref<MyTaskItem[]>([]);
const taskTotal = ref(0);

function myTasksQueryKey(
  resolvedTenantId: string,
  filter: MyTasksTab,
  queryLimit: number,
  queryOffset: number,
) {
  return [
    'get',
    TASKS_PATH,
    {
      params: {
        path: { tenant_id: resolvedTenantId },
        query: { filter, timezone, limit: queryLimit, offset: queryOffset },
      },
    },
  ] as const;
}

const tasksQuery = useQuery({
  queryKey: computed(() =>
    myTasksQueryKey(tenantId.value!, activeFilter.value, limit.value, offset.value),
  ),
  queryFn: async ({ queryKey, signal }) => {
    const { data, error } = await fetchClient.GET(TASKS_PATH, {
      params: queryKey[2].params,
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value),
  placeholderData: (previousData, previousQuery) => {
    const previousTenantId = previousQuery?.queryKey[2]?.params.path.tenant_id;
    if (previousTenantId === tenantId.value) return keepPreviousData(previousData);
    return undefined;
  },
});

watch(
  activeFilter,
  () => {
    offset.value = 0;
    limit.value = MY_TASKS_PAGE_LIMIT;
  },
  { flush: 'sync' },
);

watch(tenantId, (_next, prev) => {
  // 初回の解決（null → ID。テナント一覧がキャッシュに無いとき）では、URL から読んだ件数を保つ。
  // 戻すと再読み込みや共有リンクで `?shown=` が 50 件に戻り、URL からも消える
  if (!prev) return;
  offset.value = 0;
  limit.value = MY_TASKS_PAGE_LIMIT;
  loadedTasks.value = [];
  taskTotal.value = 0;
});

watch(
  () => tasksQuery.data.value,
  (data) => {
    if (!data || tasksQuery.isPlaceholderData.value) return;
    loadedTasks.value =
      offset.value === 0
        ? data.tasks
        : [...loadedTasks.value.slice(0, offset.value), ...data.tasks];
    taskTotal.value = data.total;
  },
  { immediate: true },
);

// 履歴は積まず、今の項目を書き換える（タスク一覧・カレンダーと同じ）
watch([activeFilter, offset, limit], () => {
  if (typeof window === 'undefined') return;
  const url = applyMyTasksUrlState(new URL(window.location.href), {
    tab: activeFilter.value,
    shown: offset.value + limit.value,
  });
  window.history.replaceState(window.history.state, '', url);
});

// 同じページへのクライアント遷移（`?tab=week` を見ている間にサイドバーの My Tasks を押す等）は
// ページが作り直されないので、URL の変化に追従する。今の状態と同じなら何もしない
watch(urlState, (next) => {
  const shown = Math.min(offset.value + limit.value, MY_TASKS_MAX_SHOWN);
  if (next.tab === activeFilter.value && next.shown === shown) return;
  activeFilter.value = next.tab;
  offset.value = 0;
  limit.value = next.shown;
});

const allTasks = computed(() => loadedTasks.value);
const hasMoreTasks = computed(() => allTasks.value.length < taskTotal.value);

function loadMore() {
  if (tasksQuery.isFetching.value || !hasMoreTasks.value) return;
  // 復元で 100 件を 1 回で取った後も、読み込んだ続きから 1 回分ずつ取る
  offset.value = loadedTasks.value.length;
  limit.value = MY_TASKS_PAGE_LIMIT;
}

const groupedTasks = computed(() => {
  const personal = allTasks.value.filter((t) => t.project.is_personal);
  const byProject = new Map<string, MyTaskItem[]>();
  for (const task of allTasks.value.filter((t) => !t.project.is_personal)) {
    const key = task.project.id;
    if (!byProject.has(key)) byProject.set(key, []);
    byProject.get(key)!.push(task);
  }
  return { personal, byProject };
});

function navigateToTask(task: MyTaskItem, event: MouseEvent) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  event.preventDefault();
  void navigate(taskDetailHref(tenantDisplayId.value, task.project.key, task.seq_id));
}

function formatDeadline(task: MyTaskItem) {
  const d = task.soft_deadline ?? task.hard_deadline;
  if (!d) return '—';
  return new Date(d).toLocaleDateString('ja-JP');
}

function priorityLabel(p: string) {
  const map: Record<string, string> = {
    CriticalFire: '🔥',
    Critical: '‼️',
    High: '⬆️',
    Medium: '➡️',
    Low: '⬇️',
    Trivial: '💤',
  };
  return map[p] ?? p;
}

function retryFailed() {
  if (isTenantResolveError.value) void tenantsQuery.refetch();
  if (tasksQuery.isError.value) void tasksQuery.refetch();
}
</script>

<template>
  <div class="mx-auto flex w-full max-w-3xl flex-col gap-6">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">My Tasks</h1>
    </div>

    <div class="flex flex-wrap gap-2">
      <Button
        v-for="tab in tabs"
        :key="tab.key"
        :variant="activeFilter === tab.key ? 'primary' : 'tertiary'"
        :label="tab.label"
        @click="activeFilter = tab.key"
      />
    </div>

    <div v-if="isTenantResolving || tasksQuery.isLoading.value" class="flex justify-center py-8">
      <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" />
    </div>

    <LoadErrorState
      v-else-if="isTenantResolveError || tasksQuery.isError.value"
      message="タスクの読み込みに失敗しました"
      @retry="retryFailed"
    />

    <p v-else-if="isTenantNotFound" class="py-8 text-center text-sm text-muted-foreground">
      テナントが見つかりません
    </p>

    <template v-else>
      <p class="text-xs text-muted-foreground" aria-live="polite">{{ taskTotal }}件</p>

      <section v-if="groupedTasks.personal.length" class="space-y-2">
        <h2 class="text-sm font-medium text-muted-foreground">個人 Inbox</h2>
        <div
          v-for="task in groupedTasks.personal"
          :key="task.id"
          class="flex items-center gap-3 rounded-md border px-3 py-2"
        >
          <a
            :href="taskDetailHref(tenantDisplayId, task.project.key, task.seq_id)"
            class="flex-1 text-link hover:underline"
            @click="navigateToTask(task, $event)"
          >
            {{ task.title }}
          </a>
          <span
            class="inline-flex shrink-0 items-center gap-1.5 rounded bg-muted px-2 py-0.5 text-xs"
            data-testid="task-status"
          >
            <span
              class="size-2 rounded-full"
              :style="{ backgroundColor: task.status.color }"
              aria-hidden="true"
            />
            {{ task.status.name }}
          </span>
          <span class="text-xs text-muted-foreground">{{ formatDeadline(task) }}</span>
          <span>{{ priorityLabel(task.priority) }}</span>
        </div>
      </section>

      <section
        v-for="[projectId, projectTasks] in groupedTasks.byProject"
        :key="projectId"
        class="space-y-2"
      >
        <h2 class="text-sm font-medium text-muted-foreground">
          {{ projectTasks[0]?.project.name }}
        </h2>
        <div
          v-for="task in projectTasks"
          :key="task.id"
          class="flex items-center gap-3 rounded-md border px-3 py-2"
        >
          <a
            :href="taskDetailHref(tenantDisplayId, task.project.key, task.seq_id)"
            class="flex-1 text-link hover:underline"
            @click="navigateToTask(task, $event)"
          >
            {{ task.title }}
          </a>
          <span
            class="inline-flex shrink-0 items-center gap-1.5 rounded bg-muted px-2 py-0.5 text-xs"
            data-testid="task-status"
          >
            <span
              class="size-2 rounded-full"
              :style="{ backgroundColor: task.status.color }"
              aria-hidden="true"
            />
            {{ task.status.name }}
          </span>
          <span class="rounded bg-muted px-2 py-0.5 text-xs">{{ task.project.key }}</span>
          <span class="text-xs text-muted-foreground">{{ formatDeadline(task) }}</span>
          <span>{{ priorityLabel(task.priority) }}</span>
        </div>
      </section>

      <p v-if="!allTasks.length" class="py-8 text-center text-sm text-muted-foreground">
        {{ myTasksEmptyMessage(activeFilter) }}
      </p>

      <div v-if="hasMoreTasks" class="flex justify-center pt-2">
        <Button
          variant="tertiary"
          :disabled="tasksQuery.isFetching.value"
          :label="tasksQuery.isFetching.value ? '読み込み中…' : 'もっと見る'"
          @click="loadMore"
        >
          <template #icon><Spinner v-if="tasksQuery.isFetching.value" hide-label /></template>
        </Button>
      </div>
    </template>
  </div>
</template>

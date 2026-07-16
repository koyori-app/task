<script setup lang="ts">
import { Check, Loader2 } from '@lucide/vue';
import { computed, ref } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';

import { Button } from '@/components/ui/button';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';
import { taskDetailHref } from '@/lib/task-display';

type FilterTab = 'today' | 'week' | 'no_due_date' | 'overdue' | 'all';
type MyTaskItem = components['schemas']['MyTaskItem'];

const TASKS_PATH = '/v1/tenants/{tenant_id}/users/me/tasks' as const;

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);

const tabs: { key: FilterTab; label: string }[] = [
  { key: 'today', label: '今日' },
  { key: 'week', label: '今週' },
  { key: 'no_due_date', label: '期限なし' },
  { key: 'overdue', label: '期限超過' },
  { key: 'all', label: 'すべて' },
];

const activeFilter = ref<FilterTab>('today');

function myTasksQueryKey(resolvedTenantId: string, filter: FilterTab) {
  return [
    'get',
    TASKS_PATH,
    { params: { path: { tenant_id: resolvedTenantId }, query: { filter } } },
  ] as const;
}

const tasksQuery = useQuery({
  queryKey: computed(() => myTasksQueryKey(tenantId.value!, activeFilter.value)),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(TASKS_PATH, {
      params: {
        path: { tenant_id: tenantId.value! },
        query: { filter: activeFilter.value },
      },
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value),
});

const allTasks = computed(() => tasksQuery.data.value?.tasks ?? []);

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
</script>

<template>
  <div class="mx-auto flex w-full max-w-3xl flex-col gap-6">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">My Tasks</h1>
      <p class="text-sm text-muted-foreground">このテナントで自分に割り当てられたタスク</p>
    </div>

    <div class="flex flex-wrap gap-2">
      <Button
        v-for="tab in tabs"
        :key="tab.key"
        size="sm"
        :variant="activeFilter === tab.key ? 'default' : 'outline'"
        @click="activeFilter = tab.key"
      >
        {{ tab.label }}
      </Button>
    </div>

    <div v-if="isTenantResolving || tasksQuery.isLoading.value" class="flex justify-center py-8">
      <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" />
    </div>

    <p
      v-else-if="isTenantResolveError || tasksQuery.isError.value"
      class="py-8 text-center text-sm text-destructive"
    >
      タスクの読み込みに失敗しました
    </p>

    <p v-else-if="isTenantNotFound" class="py-8 text-center text-sm text-muted-foreground">
      テナントが見つかりません
    </p>

    <template v-else>
      <p class="text-xs text-muted-foreground" aria-live="polite">{{ allTasks.length }}件</p>

      <section v-if="groupedTasks.personal.length" class="space-y-2">
        <h2 class="text-sm font-medium text-muted-foreground">個人 Inbox</h2>
        <div
          v-for="task in groupedTasks.personal"
          :key="task.id"
          class="flex items-center gap-3 rounded-md border px-3 py-2"
        >
          <Check class="h-4 w-4 text-muted-foreground" />
          <a
            :href="taskDetailHref(tenantDisplayId, task.project.key, task.seq_id)"
            class="flex-1 text-primary hover:underline"
            @click="navigateToTask(task, $event)"
          >
            {{ task.title }}
          </a>
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
          <Check class="h-4 w-4 text-muted-foreground" />
          <a
            :href="taskDetailHref(tenantDisplayId, task.project.key, task.seq_id)"
            class="flex-1 text-primary hover:underline"
            @click="navigateToTask(task, $event)"
          >
            {{ task.title }}
          </a>
          <span class="rounded bg-muted px-2 py-0.5 text-xs">{{ task.project.key }}</span>
          <span class="text-xs text-muted-foreground">{{ formatDeadline(task) }}</span>
          <span>{{ priorityLabel(task.priority) }}</span>
        </div>
      </section>

      <p v-if="!allTasks.length" class="py-8 text-center text-sm text-muted-foreground">
        割り当てられたタスクは0件です
      </p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { Check, Loader2 } from '@lucide/vue';
import { computed, ref } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { usePageContext } from 'vike-vue/usePageContext';

import { Button } from '@/components/ui/button';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { components } from '@/generated/api';
import { fetchClient, apiClient } from '@/lib/api-vue-query';

type FilterTab = 'today' | 'week' | 'no_due_date' | 'overdue' | 'all';

interface MyTaskItem {
  id: string;
  seq_key: string;
  title: string;
  priority: string;
  soft_deadline: string | null;
  hard_deadline: string | null;
  is_personal: boolean;
  project: { id: string; name: string; key: string; is_personal: boolean };
  status: { id: string; name: string; color: string; is_done_state?: boolean };
}

const TASKS_PATH = '/v1/tenants/{tenant_id}/users/me/tasks' as const;

const pageContext = usePageContext();
const tenantId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const queryClient = useQueryClient();

const tabs: { key: FilterTab; label: string }[] = [
  { key: 'today', label: '今日' },
  { key: 'week', label: '今週' },
  { key: 'no_due_date', label: '期限なし' },
  { key: 'overdue', label: '期限超過' },
  { key: 'all', label: 'すべて' },
];

const activeFilter = ref<FilterTab>('today');
const captureTitle = ref('');
const captureDeadline = ref('');
const capturePriority = ref<components['schemas']['TaskPriority']>('Medium');
const errorMessage = ref<string | null>(null);

const tasksQuery = useQuery({
  queryKey: computed(() => [
    'get',
    TASKS_PATH,
    { params: { path: { tenant_id: tenantId.value }, query: { filter: activeFilter.value } } },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(TASKS_PATH, {
      // generated type incorrectly puts filter in path; pass as query at runtime
      params: { path: { tenant_id: tenantId.value }, query: { filter: activeFilter.value } } as any,
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value),
});

const allTasks = computed(() => (tasksQuery.data.value?.tasks ?? []) as MyTaskItem[]);

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

const captureMutation = apiClient.useMutation('post', TASKS_PATH, {
  onSuccess: () => {
    queryClient.invalidateQueries({ queryKey: ['get', TASKS_PATH] });
  },
});

async function submitCapture() {
  if (!captureTitle.value.trim() || !tenantId.value) return;
  const body: components['schemas']['QuickCaptureRequest'] = {
    title: captureTitle.value.trim(),
    priority: capturePriority.value,
  };
  if (captureDeadline.value) {
    body.soft_deadline = new Date(`${captureDeadline.value}T00:00:00`).toISOString();
  }
  errorMessage.value = null;
  try {
    await captureMutation.mutateAsync({
      params: { path: { tenant_id: tenantId.value } },
      body,
    });
    captureTitle.value = '';
    captureDeadline.value = '';
  } catch {
    errorMessage.value = 'タスクの追加に失敗しました';
  }
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
      <p class="text-sm text-muted-foreground">自分に割り当てられたタスクをテナント横断で管理</p>
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

    <HydrationSafeForm
      v-slot="{ isHydrated }"
      class="flex flex-col gap-3 rounded-lg border p-4"
      @submit="submitCapture"
    >
      <p class="text-sm font-medium">クイックキャプチャ</p>
      <div class="flex flex-col gap-2 sm:flex-row">
        <Input v-model="captureTitle" placeholder="タスクを追加..." class="flex-1" />
        <Input v-model="captureDeadline" type="date" class="w-full sm:w-40" />
        <Select v-model="capturePriority">
          <SelectTrigger class="w-full sm:w-24">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="High">高</SelectItem>
            <SelectItem value="Medium">中</SelectItem>
            <SelectItem value="Low">低</SelectItem>
          </SelectContent>
        </Select>
        <Button
          type="submit"
          :disabled="captureMutation.isPending.value || !captureTitle.trim() || !isHydrated"
        >
          <Loader2 v-if="captureMutation.isPending.value" class="mr-2 h-4 w-4 animate-spin" />
          追加
        </Button>
      </div>
      <p v-if="errorMessage" class="text-sm text-destructive mt-2">{{ errorMessage }}</p>
    </HydrationSafeForm>

    <div v-if="tasksQuery.isFetching.value" class="flex justify-center py-8">
      <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" />
    </div>

    <p v-else-if="tasksQuery.isError.value" class="py-8 text-center text-sm text-destructive">
      タスクの読み込みに失敗しました
    </p>

    <template v-else>
      <section v-if="groupedTasks.personal.length" class="space-y-2">
        <h2 class="text-sm font-medium text-muted-foreground">個人 Inbox</h2>
        <div
          v-for="task in groupedTasks.personal"
          :key="task.id"
          class="flex items-center gap-3 rounded-md border px-3 py-2"
        >
          <Check class="h-4 w-4 text-muted-foreground" />
          <span class="flex-1">{{ task.title }}</span>
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
          <span class="flex-1">{{ task.title }}</span>
          <span class="rounded bg-muted px-2 py-0.5 text-xs">{{ task.project.key }}</span>
          <span class="text-xs text-muted-foreground">{{ formatDeadline(task) }}</span>
          <span>{{ priorityLabel(task.priority) }}</span>
        </div>
      </section>

      <p v-if="!allTasks.length" class="py-8 text-center text-sm text-muted-foreground">
        タスクがありません
      </p>
    </template>
  </div>
</template>

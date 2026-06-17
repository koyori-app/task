<script setup lang="ts">
import { Check, Loader2 } from '@lucide/vue';
import { computed, onMounted, ref } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { apiClient } from '@/lib/api';

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

const pageContext = usePageContext();
const tenantId = computed(() => String(pageContext.routeParams.tenant ?? ''));

const tabs: { key: FilterTab; label: string }[] = [
  { key: 'today', label: '今日' },
  { key: 'week', label: '今週' },
  { key: 'no_due_date', label: '期限なし' },
  { key: 'overdue', label: '期限超過' },
  { key: 'all', label: 'すべて' },
];

const activeFilter = ref<FilterTab>('today');
const tasks = ref<MyTaskItem[]>([]);
const loading = ref(false);
const captureTitle = ref('');
const captureDeadline = ref('');
const capturePriority = ref('medium');
const submitting = ref(false);
const errorMessage = ref<string | null>(null);

const groupedTasks = computed(() => {
  const personal = tasks.value.filter((t) => t.project.is_personal);
  const byProject = new Map<string, MyTaskItem[]>();
  for (const task of tasks.value.filter((t) => !t.project.is_personal)) {
    const key = task.project.id;
    if (!byProject.has(key)) byProject.set(key, []);
    byProject.get(key)!.push(task);
  }
  return { personal, byProject };
});

async function loadTasks() {
  if (!tenantId.value) return;
  loading.value = true;
  try {
    const { data, error } = await apiClient.GET('/v1/tenants/{tenant_id}/users/me/tasks', {
      params: { path: { tenant_id: tenantId.value }, query: { filter: activeFilter.value } },
    });
    if (error) throw error;
    tasks.value = (data?.tasks ?? []) as MyTaskItem[];
    errorMessage.value = null;
  } catch {
    errorMessage.value = 'タスクの読み込みに失敗しました';
  } finally {
    loading.value = false;
  }
}

async function submitCapture() {
  if (!captureTitle.value.trim() || !tenantId.value) return;
  submitting.value = true;
  try {
    const body: Record<string, unknown> = {
      title: captureTitle.value.trim(),
      priority: capturePriority.value,
    };
    if (captureDeadline.value) {
      body.soft_deadline = new Date(`${captureDeadline.value}T00:00:00`).toISOString();
    }
    const { error } = await apiClient.POST('/v1/tenants/{tenant_id}/users/me/tasks', {
      params: { path: { tenant_id: tenantId.value } },
      body: body as never,
    });
    if (error) throw error;
    captureTitle.value = '';
    captureDeadline.value = '';
    errorMessage.value = null;
    await loadTasks();
  } catch {
    errorMessage.value = 'タスクの追加に失敗しました';
  } finally {
    submitting.value = false;
  }
}

function formatDeadline(task: MyTaskItem) {
  const d = task.soft_deadline ?? task.hard_deadline;
  if (!d) return '—';
  return new Date(d).toLocaleDateString('ja-JP');
}

function priorityLabel(p: string) {
  const map: Record<string, string> = {
    critical_fire: '🔥',
    critical: '‼️',
    high: '⬆️',
    medium: '➡️',
    low: '⬇️',
    trivial: '💤',
  };
  return map[p] ?? p;
}

onMounted(loadTasks);
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
        @click="
          activeFilter = tab.key;
          loadTasks();
        "
      >
        {{ tab.label }}
      </Button>
    </div>

    <form class="flex flex-col gap-3 rounded-lg border p-4" @submit.prevent="submitCapture">
      <p class="text-sm font-medium">クイックキャプチャ</p>
      <div class="flex flex-col gap-2 sm:flex-row">
        <Input v-model="captureTitle" placeholder="タスクを追加..." class="flex-1" />
        <Input v-model="captureDeadline" type="date" class="w-full sm:w-40" />
        <Select v-model="capturePriority">
          <SelectTrigger class="w-full sm:w-24">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="high">高</SelectItem>
            <SelectItem value="medium">中</SelectItem>
            <SelectItem value="low">低</SelectItem>
          </SelectContent>
        </Select>
        <Button type="submit" :disabled="submitting || !captureTitle.trim()">
          <Loader2 v-if="submitting" class="mr-2 h-4 w-4 animate-spin" />
          追加
        </Button>
      </div>
      <p v-if="errorMessage" class="text-sm text-destructive mt-2">{{ errorMessage }}</p>
    </form>

    <div v-if="loading" class="flex justify-center py-8">
      <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" />
    </div>

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

      <p v-if="!tasks.length" class="py-8 text-center text-sm text-muted-foreground">
        タスクがありません
      </p>
    </template>
  </div>
</template>

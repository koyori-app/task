<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import type { components } from '@/generated/api';
import AvatarGroup from '@/components/AvatarGroup.vue';
import { PRIORITY_CONFIG, formatDeadline, formatTaskDate, taskSeqKey } from '@/lib/task-display';

type TaskDetail = components['schemas']['TaskDetailResponse'];
type StatusOption = components['schemas']['ProjectStatusResponse'];

const props = defineProps<{
  task: TaskDetail | null;
  projectKey: string;
  statuses: StatusOption[];
  statusId: string;
  statusUpdating?: boolean;
  statusError?: string | null;
  loading?: boolean;
  notFound?: boolean;
  error?: boolean;
}>();

const emit = defineEmits<{
  'update:statusId': [value: string];
}>();

const resolvedStatus = computed(() =>
  props.statuses.find((status) => status.id === props.statusId),
);
</script>

<template>
  <div class="flex flex-col gap-6">
    <div v-if="loading" class="flex justify-center py-16">
      <Loader2 class="h-8 w-8 animate-spin text-muted-foreground" />
    </div>

    <div
      v-else-if="error"
      class="rounded-lg border border-destructive/30 bg-destructive/5 p-6 text-sm text-destructive"
    >
      タスクの読み込みに失敗しました
    </div>

    <div v-else-if="notFound" class="rounded-lg border p-6 text-sm text-muted-foreground">
      タスクが見つかりません
    </div>

    <template v-else-if="task">
      <header class="flex flex-col gap-2 border-b pb-4">
        <div class="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <slot name="breadcrumb" />
          <span class="font-mono">{{ taskSeqKey(projectKey, task.seq_id) }}</span>
        </div>
        <h1 class="text-2xl font-semibold tracking-tight">{{ task.title }}</h1>
        <slot name="header-actions" />
      </header>

      <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <main class="flex flex-col gap-6 lg:col-span-2">
          <section class="rounded-lg border p-4">
            <h2 class="mb-2 text-sm font-medium text-muted-foreground">説明</h2>
            <p v-if="task.description" class="whitespace-pre-wrap text-sm leading-relaxed">
              {{ task.description }}
            </p>
            <p v-else class="text-sm text-muted-foreground">説明はありません</p>
          </section>

          <slot name="main" />
        </main>

        <aside class="flex flex-col gap-4">
          <section class="rounded-lg border p-4">
            <h2 class="mb-3 text-sm font-medium text-muted-foreground">ステータス</h2>
            <select
              aria-label="ステータス"
              class="flex h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              :value="statusId"
              :disabled="statusUpdating"
              @change="emit('update:statusId', ($event.target as HTMLSelectElement).value)"
            >
              <option v-for="status in statuses" :key="status.id" :value="status.id">
                {{ status.name }}
              </option>
            </select>
            <p v-if="statusError" class="mt-2 text-xs text-destructive">{{ statusError }}</p>
            <p
              v-else-if="resolvedStatus"
              class="mt-2 inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium"
              :style="{
                backgroundColor: resolvedStatus.color + '1a',
                borderColor: resolvedStatus.color + '66',
                color: resolvedStatus.color,
              }"
            >
              {{ resolvedStatus.name }}
            </p>
          </section>

          <section class="rounded-lg border p-4">
            <h2 class="mb-3 text-sm font-medium text-muted-foreground">優先度</h2>
            <div
              class="inline-flex items-center gap-1.5 text-sm"
              :style="{ color: PRIORITY_CONFIG[task.priority].color }"
            >
              <component :is="PRIORITY_CONFIG[task.priority].icon" class="size-4" />
              {{ PRIORITY_CONFIG[task.priority].label }}
            </div>
          </section>

          <section class="rounded-lg border p-4">
            <h2 class="mb-3 text-sm font-medium text-muted-foreground">担当者</h2>
            <AvatarGroup
              v-if="task.assignees.length"
              :users="task.assignees.map((a) => a.user)"
              :max-display="5"
            />
            <p v-else class="text-sm text-muted-foreground">未割当</p>
          </section>

          <section class="rounded-lg border p-4">
            <h2 class="mb-3 text-sm font-medium text-muted-foreground">日付</h2>
            <dl class="space-y-2 text-sm">
              <div v-if="formatDeadline(task.soft_deadline)" class="flex justify-between gap-2">
                <dt class="text-muted-foreground">期限</dt>
                <dd
                  :class="
                    formatDeadline(task.soft_deadline)!.overdue ? 'text-red-500 font-medium' : ''
                  "
                >
                  {{ formatDeadline(task.soft_deadline)!.label }}
                </dd>
              </div>
              <div class="flex justify-between gap-2">
                <dt class="text-muted-foreground">作成</dt>
                <dd>{{ formatTaskDate(task.created_at) }}</dd>
              </div>
              <div class="flex justify-between gap-2">
                <dt class="text-muted-foreground">更新</dt>
                <dd>{{ formatTaskDate(task.updated_at) }}</dd>
              </div>
            </dl>
          </section>

          <slot name="sidebar" />
        </aside>
      </div>

      <footer>
        <slot name="footer" />
      </footer>
    </template>
  </div>
</template>

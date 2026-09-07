<script setup lang="ts">
import { ArrowUpLeft, GitBranch, Loader2, Plus, RotateCcw } from '@lucide/vue';
import { computed, ref } from 'vue';

import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';
import { Button } from '@/components/ui/button';
import { useTaskSubtasks } from '@/composables/useTaskSubtasks';
import type { components } from '@/generated/api';
import { taskSeqKey } from '@/lib/task-display';

type TaskResponse = components['schemas']['TaskResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

const props = defineProps<{
  tenantId: string | null | undefined;
  projectId: string | null | undefined;
  /** UUID または PROJECT-42。relations API の解決に使う。 */
  taskId: string;
  /** 作成する子へ設定する親 UUID。 */
  taskUuid: string;
  statusId: string;
  statusUpdating: boolean;
  statuses: StatusResponse[];
  projectKey: string;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
}>();

const adding = ref(false);
const subtasks = useTaskSubtasks({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.taskId,
  taskUuid: () => props.taskUuid,
});

const statusesById = computed(() => new Map(props.statuses.map((status) => [status.id, status])));

function openComposer() {
  if (props.statusUpdating) return;
  adding.value = true;
}

function cancelComposer() {
  if (props.statusUpdating) return;
  adding.value = false;
}

function createSubtask(title: string) {
  if (props.statusUpdating) return Promise.resolve(false);
  return subtasks.createSubtask(title, props.statusId);
}
</script>

<template>
  <div class="border-t pt-5" data-task-subtasks>
    <button
      v-if="subtasks.parentTask.value"
      type="button"
      class="mb-4 flex max-w-full items-center gap-2 rounded px-1 py-1 text-sm text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
      @click="emit('open', subtasks.parentTask.value!)"
    >
      <ArrowUpLeft class="size-4 shrink-0" aria-hidden="true" />
      <span class="shrink-0">親タスク</span>
      <span class="font-mono text-xs">
        {{ taskSeqKey(projectKey, subtasks.parentTask.value!.seq_id) }}
      </span>
      <span class="truncate">{{ subtasks.parentTask.value!.title }}</span>
    </button>

    <div class="mb-2 flex items-center gap-2">
      <GitBranch class="size-4 text-muted-foreground" aria-hidden="true" />
      <h2 class="text-sm font-medium text-muted-foreground">サブタスク</h2>
      <span
        v-if="!subtasks.loading.value && !subtasks.error.value"
        class="text-xs tabular-nums text-muted-foreground"
      >
        {{ subtasks.subtasks.value.length }}
      </span>
    </div>

    <div
      v-if="subtasks.loading.value"
      class="flex items-center gap-2 py-2 text-sm text-muted-foreground"
    >
      <Loader2 class="size-4 animate-spin" aria-hidden="true" />
      読み込み中…
    </div>

    <div v-else-if="subtasks.error.value" class="flex items-center gap-2 py-1">
      <p class="text-sm text-destructive">サブタスクを読み込めませんでした</p>
      <Button
        type="button"
        variant="outline"
        size="sm"
        class="h-7 gap-1.5"
        @click="subtasks.refetch"
      >
        <RotateCcw class="size-3.5" aria-hidden="true" />
        再試行
      </Button>
    </div>

    <template v-else>
      <ul v-if="subtasks.subtasks.value.length" class="mb-2 border-y border-border/60">
        <li
          v-for="subtask in subtasks.subtasks.value"
          :key="subtask.id"
          class="border-b border-border/60 last:border-b-0"
        >
          <button
            type="button"
            class="flex w-full min-w-0 items-center gap-2 px-1 py-2 text-left transition-colors hover:bg-muted/40"
            @click="emit('open', subtask)"
          >
            <span
              class="size-2.5 shrink-0 rounded-full border"
              :style="
                statusesById.get(subtask.status_id)?.color
                  ? { backgroundColor: statusesById.get(subtask.status_id)?.color }
                  : undefined
              "
              aria-hidden="true"
            />
            <span class="shrink-0 font-mono text-xs text-muted-foreground">
              {{ taskSeqKey(projectKey, subtask.seq_id) }}
            </span>
            <span class="truncate text-sm">{{ subtask.title }}</span>
            <span class="ml-auto shrink-0 text-xs text-muted-foreground">
              {{ statusesById.get(subtask.status_id)?.name ?? '—' }}
            </span>
          </button>
        </li>
      </ul>
      <p v-else-if="!adding" class="py-1 text-sm text-muted-foreground">サブタスクはありません</p>

      <TaskSubtaskComposer
        v-if="adding"
        :pending="subtasks.createPending.value"
        :disabled="statusUpdating"
        :error="subtasks.createError.value"
        :on-create="createSubtask"
        @cancel="cancelComposer"
      />
      <Button
        v-else
        type="button"
        variant="ghost"
        size="sm"
        class="h-8 gap-2 px-2 text-sm font-normal text-muted-foreground"
        :disabled="statusUpdating"
        @click="openComposer"
      >
        <Plus class="size-4" aria-hidden="true" />
        サブタスクを追加
      </Button>
    </template>
  </div>
</template>

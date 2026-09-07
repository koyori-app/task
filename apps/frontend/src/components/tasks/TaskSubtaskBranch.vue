<script setup lang="ts">
import { Loader2, Plus, RotateCcw } from '@lucide/vue';
import { ref } from 'vue';

import TaskGroupedRow from '@/components/tasks/TaskGroupedRow.vue';
import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';
import { Button } from '@/components/ui/button';
import { useTaskSubtasks } from '@/composables/useTaskSubtasks';
import type { TaskRowField } from '@/composables/useTaskRowMutations';
import type { components } from '@/generated/api';

type TaskResponse = components['schemas']['TaskResponse'];
type LabelResponse = components['schemas']['LabelResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];
type ProjectMember = { id: string; username: string; avatar_url?: string | null };

const props = defineProps<{
  parentTask: TaskResponse;
  tenantId: string | null | undefined;
  projectId: string | null | undefined;
  statuses: StatusResponse[];
  projectLabels: LabelResponse[];
  members: ProjectMember[];
  membersState?: { loading?: boolean; error?: boolean; onRetry?: () => void };
  pending: Record<string, TaskRowField | undefined>;
  errors: Record<string, string | undefined>;
  commentPendingTaskIds?: Record<string, boolean>;
  onComment: (task: TaskResponse, body: string) => Promise<boolean>;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
  collapse: [];
  'update:status': [task: TaskResponse, statusId: string];
  'update:priority': [task: TaskResponse, priority: TaskResponse['priority']];
  'update:softDeadline': [task: TaskResponse, iso: string | null];
  'toggle:assignee': [task: TaskResponse, userId: string, checked: boolean];
  'toggle:label': [task: TaskResponse, labelId: string, checked: boolean];
}>();

const adding = ref(false);
const subtasks = useTaskSubtasks({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.parentTask.id,
  taskUuid: () => props.parentTask.id,
});

function cancelComposer() {
  if (!subtasks.subtasks.value.length) {
    emit('collapse');
    return;
  }
  adding.value = false;
}
</script>

<template>
  <div class="ml-5 border-l border-border/70 bg-muted/10" data-subtask-branch>
    <div
      v-if="subtasks.loading.value"
      class="flex min-w-[42rem] items-center gap-2 px-3 py-2 text-sm text-muted-foreground"
    >
      <Loader2 class="size-4 animate-spin" aria-hidden="true" />
      サブタスクを読み込み中…
    </div>

    <div v-else-if="subtasks.error.value" class="flex min-w-[42rem] items-center gap-2 px-3 py-2">
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
      <TaskGroupedRow
        v-for="subtask in subtasks.subtasks.value"
        :key="subtask.id"
        :task="subtask"
        :statuses="statuses"
        :project-labels="projectLabels"
        :members="members"
        :members-state="membersState"
        :pending-field="pending[subtask.id]"
        :error="errors[subtask.id]"
        :comment-pending="!!commentPendingTaskIds?.[subtask.id]"
        :show-subtask-toggle="false"
        :depth="1"
        :on-comment="(body: string) => onComment(subtask, body)"
        @open="emit('open', subtask)"
        @update:status="(statusId) => emit('update:status', subtask, statusId)"
        @update:priority="(priority) => emit('update:priority', subtask, priority)"
        @update:soft-deadline="(iso) => emit('update:softDeadline', subtask, iso)"
        @toggle:assignee="(userId, checked) => emit('toggle:assignee', subtask, userId, checked)"
        @toggle:label="(labelId, checked) => emit('toggle:label', subtask, labelId, checked)"
      />

      <div class="min-w-[42rem] px-3 py-1.5">
        <!-- 空なら展開した時点で作成欄を出す。既存の子があるときは明示操作で開く。 -->
        <TaskSubtaskComposer
          v-if="adding || !subtasks.subtasks.value.length"
          :pending="subtasks.createPending.value"
          :error="subtasks.createError.value"
          :aria-label="`${parentTask.title} のサブタスク名`"
          :on-create="(title) => subtasks.createSubtask(title, parentTask.status_id)"
          @cancel="cancelComposer"
        />
        <Button
          v-else
          type="button"
          variant="ghost"
          size="sm"
          class="h-7 gap-1.5 px-2 text-xs text-muted-foreground"
          @click="adding = true"
        >
          <Plus class="size-3.5" aria-hidden="true" />
          サブタスクを追加
        </Button>
      </div>
    </template>
  </div>
</template>

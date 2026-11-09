<script setup lang="ts">
import { Loader2, Plus, RotateCcw } from '@lucide/vue';
import { computed, ref, watch } from 'vue';

import TaskGroupedRow from '@/components/tasks/TaskGroupedRow.vue';
import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';
import { Button } from '@koyori-app/ui-vue';
import { useTaskSubtasks, type TaskSubtaskFilters } from '@/composables/useTaskSubtasks';
import type { TaskRowField } from '@/composables/useTaskRowMutations';
import type { components } from '@/generated/api';

type TaskResponse = components['schemas']['TaskResponse'];
type LabelResponse = components['schemas']['LabelResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];
type ProjectMember = { id: string; username: string; avatar_url?: string | null };

const props = defineProps<{
  parentTask: TaskResponse;
  /** タスク ID（`KEY-N`）を組み立てるプロジェクトキー */
  projectKey: string;
  filters?: TaskSubtaskFilters;
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
  /**
   * 環境設定で開いた枝。利用者が開いた枝と違い、子が 0 件なら作成欄を出さず何も描かない。
   * 読み込み中の表示も出さない（行ごとに並ぶため）。失敗は出す
   */
  implicit?: boolean;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
  collapse: [];
  /** 取得が終わるたびに、子が 0 件かを返す（暗黙の枝を描くかの判断は親に任せる） */
  empty: [empty: boolean];
  'update:status': [task: TaskResponse, statusId: string];
  'update:priority': [task: TaskResponse, priority: TaskResponse['priority']];
  'update:softDeadline': [task: TaskResponse, iso: string | null];
  'toggle:assignee': [task: TaskResponse, userId: string, checked: boolean];
  'toggle:label': [task: TaskResponse, labelId: string, checked: boolean];
}>();

const adding = ref(false);
const statusUpdating = computed(() => props.pending[props.parentTask.id] === 'status_id');
const subtasks = useTaskSubtasks({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.parentTask.id,
  taskUuid: () => props.parentTask.id,
  parentTaskId: () => props.parentTask.parent_task_id,
  filters: () => ({ is_archived: false, ...props.filters }),
});

// 暗黙の枝は子が無いと何も描かない。親の矢印を閉じた見た目にするため、子の有無を返す
const hidden = computed(
  () =>
    !!props.implicit &&
    !subtasks.loading.value &&
    !subtasks.error.value &&
    !subtasks.subtasks.value.length,
);

watch(
  [() => subtasks.loading.value, () => subtasks.subtasks.value.length],
  ([loading, count]) => {
    if (!loading) emit('empty', count === 0);
  },
  { immediate: true },
);

function openComposer() {
  if (props.parentTask.parent_task_id || statusUpdating.value) return;
  adding.value = true;
}

function cancelComposer() {
  if (statusUpdating.value) return;
  if (!subtasks.subtasks.value.length) {
    emit('collapse');
    return;
  }
  adding.value = false;
}

function createSubtask(title: string) {
  if (props.parentTask.parent_task_id || statusUpdating.value) return Promise.resolve(false);
  return subtasks.createSubtask(title, props.parentTask.status_id);
}
</script>

<template>
  <div v-if="!hidden" class="ml-5 border-l border-border/70 bg-muted/10" data-subtask-branch>
    <div
      v-if="subtasks.loading.value && !implicit"
      class="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground"
    >
      <Loader2 class="size-4 animate-spin" aria-hidden="true" />
      サブタスクを読み込み中…
    </div>

    <!-- 暗黙の枝は読み込み中に何も出さない -->
    <template v-else-if="subtasks.loading.value" />

    <div v-else-if="subtasks.error.value" class="flex items-center gap-2 px-3 py-2">
      <p class="text-sm text-destructive">サブタスクを読み込めませんでした</p>
      <Button type="button" variant="tertiary" label="再試行" @click="subtasks.refetch">
        <template #icon><RotateCcw class="size-3.5" aria-hidden="true" /></template>
      </Button>
    </div>

    <template v-else>
      <!-- 枝の最後の行（入れ子で追加行が無いとき）は、枝の終わりと線が重なるので下の線を消す -->
      <TaskGroupedRow
        v-for="subtask in subtasks.subtasks.value"
        :key="subtask.id"
        class="last:border-b-0"
        :project-key="projectKey"
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

      <div v-if="!parentTask.parent_task_id" class="px-3 py-1.5">
        <!-- 空なら展開した時点で作成欄を出す。既存の子があるときは明示操作で開く。 -->
        <TaskSubtaskComposer
          v-if="adding || !subtasks.subtasks.value.length"
          :pending="subtasks.createPending.value"
          :disabled="statusUpdating"
          :error="subtasks.createError.value"
          :aria-label="`${parentTask.title} のサブタスク名`"
          :on-create="createSubtask"
          @cancel="cancelComposer"
        />
        <Button
          v-else
          type="button"
          variant="ghost"
          :disabled="statusUpdating"
          label="サブタスクを追加"
          @click="openComposer"
        >
          <template #icon><Plus class="size-3.5" aria-hidden="true" /></template>
        </Button>
      </div>
    </template>
  </div>
</template>

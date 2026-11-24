<script setup lang="ts">
import { CalendarDays, Flag } from '@lucide/vue';
import { Dropdown } from '@koyori-app/ui-vue';
import { computed } from 'vue';

import UserAvatar from '@/components/UserAvatar.vue';
import TaskStatusIcon from '@/components/tasks/TaskStatusIcon.vue';
import TaskTitleLink from '@/components/tasks/TaskTitleLink.vue';
import type { components } from '@/generated/api';
import { PRIORITY_CONFIG, formatDeadline, taskSeqKey } from '@/lib/task-display';
import { statusFill, statusMenuItems } from '@/lib/task-status-icon';

type TaskResponse = components['schemas']['TaskResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

/**
 * タスクの一覧の 1 行（サブタスク・似ているタスク）。枠と内側の余白を持たず、本文の左右端に
 * 文字を合わせる。ホバーの背景だけを左右へはみ出させる。
 *
 * 題と ID のブロックがタスクのホバーカードの起点（`data-task-row-card`）。ステータスのメニューと
 * カードが重ならないよう、アイコンと右の列は起点に入れない。
 * 狭い器（ペイン）では期限と優先度を隠すので、並べる側が `@container` を持つ。
 */
const props = defineProps<{
  task: TaskResponse;
  projectKey: string;
  tenantDisplayId: string;
  statuses: StatusResponse[];
  /** この行の更新が飛行中。ステータスのメニューを止める */
  pending?: boolean;
  /** 直近の失敗。行の下に出す */
  error?: string | null;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
  'update:status': [task: TaskResponse, statusId: string];
}>();

const seqKey = computed(() => taskSeqKey(props.projectKey, props.task.seq_id));
const status = computed(() => props.statuses.find((s) => s.id === props.task.status_id));
const fill = computed(() => statusFill(props.statuses, props.task.status_id));
const statusItems = computed(() =>
  statusMenuItems(props.statuses, props.task.status_id, { disabled: props.pending }),
);
const deadline = computed(() =>
  formatDeadline(props.task.soft_deadline ?? props.task.hard_deadline, {
    done: status.value?.is_done_state ?? !!props.task.completed_at,
    completedAt: props.task.completed_at,
  }),
);
const priority = computed(() => PRIORITY_CONFIG[props.task.priority]);
const assignees = computed(() => props.task.assignees.map((assignee) => assignee.user));

function onSelectStatus(statusId: string) {
  if (props.pending || statusId === props.task.status_id) return;
  emit('update:status', props.task, statusId);
}
</script>

<template>
  <div data-task-row :data-task-seq-key="seqKey">
    <div
      class="-mx-2 flex min-h-12 items-center gap-2 rounded-md px-2 py-1.5 transition-colors hover:bg-muted/40"
      :class="{ 'text-muted-foreground': status?.is_done_state }"
    >
      <!-- ホバーで名前。状態はアイコンで出し、名前は読み上げ名に持たせる -->
      <span class="shrink-0" :title="status?.name">
        <Dropdown
          top-layer
          :label="`${seqKey} のステータス: ${status?.name ?? '未設定'}`"
          :icon="null"
          :disabled="pending"
          :items="statusItems"
          :on-select="onSelectStatus"
        >
          <template #trigger>
            <TaskStatusIcon :status="status" :fill="fill" />
          </template>
        </Dropdown>
      </span>

      <div class="min-w-0 flex-1" data-task-row-card :data-task-id="task.id">
        <TaskTitleLink
          class="font-medium"
          :tenant-display-id="tenantDisplayId"
          :project-key="projectKey"
          :seq-id="task.seq_id"
          :title="task.title"
          @select="emit('open', task)"
        />
        <div class="mt-0.5 flex min-w-0 items-center gap-2">
          <span class="shrink-0 font-mono text-xs text-muted-foreground">{{ seqKey }}</span>
          <span
            v-for="label in task.labels"
            :key="label.id"
            class="inline-flex min-w-0 items-center gap-1.5 rounded-md bg-muted px-1.5 text-xs"
            data-task-row-label
          >
            <span
              class="inline-block size-2 shrink-0 rounded-full"
              :style="{ backgroundColor: label.color }"
              aria-hidden="true"
            />
            <span class="truncate">{{ label.name }}</span>
          </span>
        </div>
      </div>

      <div
        class="flex w-18 shrink-0 items-center"
        role="group"
        :aria-label="
          assignees.length ? `担当者: ${assignees.map((u) => u.username).join(', ')}` : '担当者なし'
        "
        data-task-row-assignees
      >
        <UserAvatar
          v-for="user in assignees.slice(0, 3)"
          :key="user.id"
          :user="user"
          size="sm"
          decorative
          class="-ml-1.5 border border-background first:ml-0"
        />
        <span v-if="assignees.length > 3" class="ml-1 text-xs text-muted-foreground">
          +{{ assignees.length - 3 }}
        </span>
      </div>
      <div
        class="hidden w-24 shrink-0 items-center gap-1 text-xs @md:flex"
        :class="deadline?.overdue ? 'text-destructive' : 'text-muted-foreground'"
        data-task-row-deadline
      >
        <template v-if="deadline">
          <CalendarDays class="size-3.5 shrink-0" aria-hidden="true" />
          <span class="sr-only">期限</span>
          <span class="truncate">{{ deadline.label }}</span>
        </template>
      </div>
      <div
        class="hidden w-16 shrink-0 items-center gap-1 text-xs text-muted-foreground @md:flex"
        data-task-row-priority
      >
        <Flag class="size-3.5 shrink-0" :style="{ color: priority.color }" aria-hidden="true" />
        <span class="sr-only">優先度</span>
        <span class="truncate">{{ priority.label }}</span>
      </div>
    </div>
    <p v-if="error" role="alert" class="pb-1 text-xs text-destructive">{{ error }}</p>
  </div>
</template>

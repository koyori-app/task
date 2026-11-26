<script setup lang="ts">
import { ArrowUpLeft, Loader2, Plus, RotateCcw } from '@lucide/vue';
import { computed, ref, useTemplateRef, watch } from 'vue';

import TaskParentPicker from '@/components/tasks/TaskParentPicker.vue';
import TaskRow from '@/components/tasks/TaskRow.vue';
import TaskRowHoverCard from '@/components/tasks/TaskRowHoverCard.vue';
import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';
import { Button } from '@koyori-app/ui-vue';
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
  parentTaskId: string | null;
  statusId: string;
  statusUpdating: boolean;
  parentUpdating: boolean;
  parentError: string | null;
  parentSaved?: boolean;
  statuses: StatusResponse[];
  projectKey: string;
  /** 行の題のリンクとホバーカードの道のテナント（表示用 id） */
  tenantDisplayId: string;
  /** 行の更新の飛行中（タスク ID ごと）。その行のステータスのメニューを止める */
  rowPending?: Record<string, unknown>;
  /** 行の更新の直近の失敗（タスク ID ごと）。行の下に出す */
  rowErrors?: Record<string, string | undefined>;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
  /** `null` で親を外す */
  'change-parent': [parentTaskId: string | null];
  /** 行のアイコンのメニューでステータスを選んだ。書き込みは消費側（行と同じ口）が行う */
  'update:status': [task: TaskResponse, statusId: string];
}>();

/** 行のホバーカードの委譲先 */
const listEl = useTemplateRef<HTMLElement>('list');

const adding = ref(false);
const pickingParent = ref(false);
const subtasks = useTaskSubtasks({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.taskId,
  taskUuid: () => props.taskUuid,
  parentTaskId: () => props.parentTaskId,
});

// 楽観更新中や取り直し中の古い relations を、今の親として描かない
const parentTask = computed(() => {
  const parent = subtasks.parentTask.value;
  return parent && parent.id === props.parentTaskId ? parent : null;
});

// 親の変更が失敗すると親付きへ戻るので、確定までは「親が無い」前提で子を作らせない
const composerBlocked = computed(() => props.statusUpdating || props.parentUpdating);

// 作成中に親が付くと、親なしの前提で作った子が孫になる
const parentChangeBlocked = computed(() => props.parentUpdating || subtasks.createPending.value);

// 親の確定で閉じる。楽観更新の間に閉じると、拒否されたときに下書きが消える
watch(
  () => !!props.parentTaskId && !props.parentUpdating,
  (isChild) => {
    if (isChild) adding.value = false;
  },
);

// オーバーレイと右ペインは別のタスクへ移っても同じインスタンス
watch(
  () => props.taskUuid,
  () => {
    pickingParent.value = false;
  },
);

const parentCandidateExclusions = computed(() => [
  props.taskUuid,
  ...(props.parentTaskId ? [props.parentTaskId] : []),
  ...subtasks.subtasks.value.map((subtask) => subtask.id),
]);

// 飛行中に次の親を選ばせないよう、送るときは候補を閉じる（ボタンは飛行中 disabled）
function changeParent(parentTaskId: string | null) {
  pickingParent.value = false;
  emit('change-parent', parentTaskId);
}

function openComposer() {
  if (props.parentTaskId || composerBlocked.value) return;
  adding.value = true;
}

function cancelComposer() {
  if (composerBlocked.value) return;
  adding.value = false;
}

function createSubtask(title: string) {
  if (props.parentTaskId || composerBlocked.value) return Promise.resolve(false);
  return subtasks.createSubtask(title, props.statusId);
}
</script>

<template>
  <div class="border-t pt-5" data-task-subtasks>
    <div class="mb-4 flex flex-wrap items-center gap-x-2 gap-y-1" data-task-parent>
      <button
        v-if="parentTask"
        type="button"
        class="flex max-w-full min-w-0 items-center gap-2 rounded px-1 py-1 text-sm text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
        @click="emit('open', parentTask!)"
      >
        <ArrowUpLeft class="size-4 shrink-0" aria-hidden="true" />
        <span class="shrink-0">親タスク</span>
        <span class="font-mono text-xs">
          {{ taskSeqKey(projectKey, parentTask.seq_id) }}
        </span>
        <span class="truncate">{{ parentTask.title }}</span>
      </button>
      <Button
        type="button"
        variant="ghost"
        :disabled="parentChangeBlocked"
        :label="parentTaskId ? '親を変更' : '親を設定'"
        @click="pickingParent = !pickingParent"
      />
      <Button
        v-if="parentTaskId"
        type="button"
        variant="ghost"
        :disabled="parentChangeBlocked"
        label="親を外す"
        @click="changeParent(null)"
      />
    </div>
    <p v-if="parentError" class="mb-4 text-xs text-destructive" role="alert">{{ parentError }}</p>
    <p v-else-if="parentSaved" class="mb-4 text-xs text-muted-foreground" role="status">
      保存しました
    </p>
    <TaskParentPicker
      v-if="pickingParent"
      :tenant-id="tenantId"
      :project-id="projectId"
      :project-key="projectKey"
      :exclude-task-ids="parentCandidateExclusions"
      :disabled="parentChangeBlocked"
      @select="changeParent"
      @cancel="pickingParent = false"
    />

    <div class="mb-1 flex min-h-8 items-center justify-between gap-2">
      <h2 class="text-sm font-medium text-muted-foreground">サブタスク</h2>
      <Button
        v-if="!parentTaskId && !adding && !subtasks.loading.value && !subtasks.error.value"
        type="button"
        variant="tertiary"
        :disabled="composerBlocked"
        label="サブタスクを追加"
        @click="openComposer"
      >
        <template #icon><Plus class="size-4" aria-hidden="true" /></template>
      </Button>
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
      <Button type="button" variant="tertiary" label="再試行" @click="subtasks.refetch">
        <template #icon><RotateCcw class="size-3.5" aria-hidden="true" /></template>
      </Button>
    </div>

    <template v-else>
      <!-- 行の区切りは行の間だけ。最後の行の下に引くと、次の節の区切りと二重になる -->
      <ul
        v-if="subtasks.subtasks.value.length"
        ref="list"
        class="@container divide-y divide-border/60"
        data-task-subtask-list
      >
        <li v-for="subtask in subtasks.subtasks.value" :key="subtask.id">
          <TaskRow
            :task="subtask"
            :project-key="projectKey"
            :tenant-display-id="tenantDisplayId"
            :statuses="statuses"
            :pending="!!rowPending?.[subtask.id]"
            :error="rowErrors?.[subtask.id]"
            @open="emit('open', $event)"
            @update:status="(task, statusId) => emit('update:status', task, statusId)"
          />
        </li>
      </ul>
      <p v-else-if="!adding" class="py-1 text-sm text-muted-foreground">サブタスクはありません</p>
      <TaskRowHoverCard
        :container="listEl"
        :tasks="subtasks.subtasks.value"
        :statuses="statuses"
        :project-key="projectKey"
        :tenant-display-id="tenantDisplayId"
      />

      <TaskSubtaskComposer
        v-if="adding"
        :pending="subtasks.createPending.value"
        :disabled="composerBlocked"
        :error="subtasks.createError.value"
        :on-create="createSubtask"
        @cancel="cancelComposer"
      />
    </template>
  </div>
</template>

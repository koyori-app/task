<script setup lang="ts">
import { ArrowUpLeft, GitBranch, Loader2, Plus, RotateCcw } from '@lucide/vue';
import { computed, ref, watch } from 'vue';

import TaskParentPicker from '@/components/tasks/TaskParentPicker.vue';
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
  statuses: StatusResponse[];
  projectKey: string;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
  /** `null` で親を外す */
  'change-parent': [parentTaskId: string | null];
}>();

const adding = ref(false);
const pickingParent = ref(false);
const subtasks = useTaskSubtasks({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.taskId,
  taskUuid: () => props.taskUuid,
  parentTaskId: () => props.parentTaskId,
});

const statusesById = computed(() => new Map(props.statuses.map((status) => [status.id, status])));

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
      <Button type="button" variant="tertiary" label="再試行" @click="subtasks.refetch">
        <template #icon><RotateCcw class="size-3.5" aria-hidden="true" /></template>
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
        :disabled="composerBlocked"
        :error="subtasks.createError.value"
        :on-create="createSubtask"
        @cancel="cancelComposer"
      />
      <Button
        v-else-if="!parentTaskId"
        type="button"
        variant="ghost"
        :disabled="composerBlocked"
        label="サブタスクを追加"
        @click="openComposer"
      >
        <template #icon><Plus class="size-4" aria-hidden="true" /></template>
      </Button>
    </template>
  </div>
</template>

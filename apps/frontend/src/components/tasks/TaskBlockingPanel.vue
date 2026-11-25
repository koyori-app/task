<script setup lang="ts">
import { Ellipsis, Loader2, Plus, RotateCcw } from '@lucide/vue';
import { GanttChart } from '@koyori-app/arc-vue';
import '@koyori-app/arc-vue/style.css';
import { computed, onErrorCaptured, ref, useTemplateRef, watch } from 'vue';

import TaskParentPicker from '@/components/tasks/TaskParentPicker.vue';
import TaskRowHoverCard from '@/components/tasks/TaskRowHoverCard.vue';
import TaskStatusIcon from '@/components/tasks/TaskStatusIcon.vue';
import { Button, Dropdown, type DropdownItem } from '@koyori-app/ui-vue';
import {
  useTaskBlocking,
  type BlockingRelationType,
  type RelationEntry,
} from '@/composables/useTaskBlocking';
import type { components } from '@/generated/api';
import { GANTT_PALETTE } from '@/lib/gantt/chart';
import { buildBlockingChart } from '@/lib/task-blocking-gantt';
import { statusFill } from '@/lib/task-status-icon';

type TaskResponse = components['schemas']['TaskResponse'];
type TaskDetail = components['schemas']['TaskDetailResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

/** 図の 1 行の高さと、日付の見出しの高さ。左のラベルの列を同じ高さで並べて行を合わせる */
const ROW_H = 32;
const HEADER_H = 32;

/**
 * ブロック関係。小さなガントで「待たせている相手 → このタスク → 待っている相手」を並べ、
 * 日付の食い違い（相手の期限がこのタスクの開始より後）を点線と注記で出す。
 *
 * 開始も期限も無いタスクは、今日の 1 日の棒で仮に出している（koyori-app/arc#82 の対応待ち）。
 * 棒の色は完了かどうかだけで、ステータスごとの色は出せない（koyori-app/arc#83）。
 * ステータスの色は左のラベルのアイコンが持つ。
 */
const props = defineProps<{
  tenantId: string | null | undefined;
  projectId: string | null | undefined;
  /** UUID または PROJECT-42。relations API の解決に使う。 */
  taskId: string;
  /** 自分自身の UUID。候補の除外と自己参照のガードに使う。 */
  taskUuid: string;
  /** このタスク（題・日付・ステータス）。図のまんなかの行になる */
  task: TaskDetail;
  statuses: StatusResponse[];
  projectKey: string;
  /** ラベルのホバーカードの道のテナント（表示用 id） */
  tenantDisplayId: string;
}>();

const emit = defineEmits<{
  open: [task: TaskResponse];
}>();

const blocking = useTaskBlocking({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  taskId: () => props.taskId,
  taskUuid: () => props.taskUuid,
});

// どちらの向きの相手を探しているか
const picking = ref<BlockingRelationType | null>(null);

// オーバーレイと右ペインは別のタスクへ移っても同じインスタンス
watch(
  () => props.taskUuid,
  () => {
    picking.value = null;
  },
);

const statusesById = computed(() => new Map(props.statuses.map((status) => [status.id, status])));

function isDone(entry: RelationEntry) {
  return statusesById.value.get(entry.status_id)?.is_done_state === true;
}

const openBlockerCount = computed(
  () => blocking.blockedBy.value.filter((entry) => !isDone(entry)).length,
);

// 逆向きも同じレコードで 409 になるので、両方向とも除く。深い循環は backend の 409 に任せる
const candidateExclusions = computed(() => [
  props.taskUuid,
  ...blocking.blocks.value.map((entry) => entry.id),
  ...blocking.blockedBy.value.map((entry) => entry.id),
]);

/** 追加は 1 つのボタンから、押してから向きを選ぶ */
const ADD_ITEMS: DropdownItem[] = [
  { value: 'blocked_by', label: 'ブロックされるタスクを追加' },
  { value: 'blocks', label: 'ブロックするタスクを追加' },
];
const REMOVE_ITEMS: DropdownItem[] = [{ value: 'remove', label: '関係を外す', destructive: true }];

function pick(type: string) {
  picking.value = type === 'blocks' ? 'blocks' : 'blocked_by';
}

// 飛行中に次の相手を選ばせないよう、送るときは候補を閉じる
function add(targetTaskId: string) {
  const type = picking.value;
  if (!type) return;
  picking.value = null;
  void blocking.addRelation(type, targetTaskId);
}

function remove(entry: RelationEntry) {
  void blocking.removeRelation(entry.relation_id);
}

/** 見る人の今日（その人の時間帯の暦の日） */
function localToday(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
const today = localToday();

const related = computed(() => [...blocking.blockedBy.value, ...blocking.blocks.value]);
const relationsById = computed(() => new Map(related.value.map((entry) => [entry.id, entry])));

const chart = computed(() =>
  buildBlockingChart({
    task: props.task,
    blockedBy: blocking.blockedBy.value,
    blocks: blocking.blocks.value,
    statuses: props.statuses,
    projectKey: props.projectKey,
    today,
  }),
);

/** ラベルのホバーカードの委譲先 */
const labelsEl = useTemplateRef<HTMLElement>('labels');

// wasm を読めないときは、arc-vue の onMounted の失敗がここへ上がる。節ごと落とさず、図だけ案内に替える。
// 左のラベルの列は図が無くても読めるので残す。ほかの子の失敗までは握らない
const chartError = ref(false);
onErrorCaptured((_error, instance) => {
  if (instance?.$.type !== GanttChart) return true;
  chartError.value = true;
  return false;
});
watch(
  () => props.taskUuid,
  () => {
    chartError.value = false;
  },
);
</script>

<template>
  <div class="border-t pt-5" data-task-blocking>
    <div class="mb-1 flex min-h-8 items-center gap-2">
      <h2 class="text-sm font-medium text-muted-foreground">ブロック関係</h2>
      <span
        v-if="!blocking.loading.value && !blocking.error.value && openBlockerCount > 0"
        class="text-xs tabular-nums text-destructive"
        data-task-blocking-open-count
      >
        未完了のブロッカー {{ openBlockerCount }} 件
      </span>
      <Dropdown
        v-if="!blocking.loading.value && !blocking.error.value"
        top-layer
        class="ml-auto"
        variant="tertiary"
        label="関係を追加"
        :disabled="blocking.mutating.value"
        :items="ADD_ITEMS"
        :on-select="pick"
      >
        <template #icon><Plus class="size-4" aria-hidden="true" /></template>
      </Dropdown>
    </div>
    <p v-if="blocking.mutationError.value" class="mb-2 text-xs text-destructive" role="alert">
      {{ blocking.mutationError.value }}
    </p>

    <div
      v-if="blocking.loading.value"
      class="flex items-center gap-2 py-2 text-sm text-muted-foreground"
    >
      <Loader2 class="size-4 animate-spin" aria-hidden="true" />
      読み込み中…
    </div>

    <div v-else-if="blocking.error.value" class="flex items-center gap-2 py-1">
      <p class="text-sm text-destructive">ブロック関係を読み込めませんでした</p>
      <Button type="button" variant="tertiary" label="再試行" @click="blocking.refetch">
        <template #icon><RotateCcw class="size-3.5" aria-hidden="true" /></template>
      </Button>
    </div>

    <template v-else>
      <TaskParentPicker
        v-if="picking"
        :tenant-id="tenantId"
        :project-id="projectId"
        :project-key="projectKey"
        :exclude-task-ids="candidateExclusions"
        :disabled="blocking.mutating.value"
        :input-label="
          picking === 'blocks' ? 'ブロックするタスクを検索' : 'ブロックされるタスクを検索'
        "
        cancel-label="ブロック相手の選択をやめる"
        @select="add"
        @cancel="picking = null"
      />

      <p v-if="!related.length" class="py-1 text-sm text-muted-foreground">
        ブロック関係はありません
      </p>
      <template v-else>
        <div class="flex min-w-0" data-task-blocking-chart>
          <!-- ラベルの列。図の見出しの高さぶん下げて、行を棒とそろえる -->
          <ul ref="labels" class="w-52 shrink-0" :style="{ paddingTop: `${HEADER_H}px` }">
            <li
              v-for="label in chart.labels"
              :key="label.task.id"
              class="flex items-center gap-1 pr-2"
              :style="{ height: `${ROW_H}px` }"
              :data-blocking-role="label.role"
            >
              <TaskStatusIcon
                :status="statusesById.get(label.task.status_id)"
                :fill="statusFill(statuses, label.task.status_id)"
              />
              <span class="sr-only">{{ statusesById.get(label.task.status_id)?.name }}</span>
              <!-- このタスク自身は開く先が無いので、押せる形にしない -->
              <span
                v-if="label.role === 'self'"
                class="flex min-w-0 flex-1 items-baseline gap-1.5 px-1"
                aria-current="true"
              >
                <span class="shrink-0 font-mono text-xs text-muted-foreground">{{
                  label.seqKey
                }}</span>
                <span class="truncate text-sm font-semibold">{{ label.task.title }}</span>
              </span>
              <template v-else>
                <button
                  type="button"
                  class="flex min-w-0 flex-1 items-baseline gap-1.5 rounded-md px-1 text-left leading-7 transition-colors hover:bg-muted/40"
                  :class="{ 'text-muted-foreground': label.done }"
                  data-task-row-card
                  :data-task-id="label.task.id"
                  @click="emit('open', relationsById.get(label.task.id)!)"
                >
                  <span class="shrink-0 font-mono text-xs text-muted-foreground">{{
                    label.seqKey
                  }}</span>
                  <span class="truncate text-sm">{{ label.task.title }}</span>
                </button>
                <Dropdown
                  top-layer
                  class="shrink-0"
                  :ariaLabel="`${label.seqKey} との関係の操作`"
                  :disabled="blocking.mutating.value"
                  :items="REMOVE_ITEMS"
                  :on-select="() => remove(relationsById.get(label.task.id)!)"
                >
                  <template #icon><Ellipsis class="size-4" aria-hidden="true" /></template>
                </Dropdown>
              </template>
            </li>
          </ul>
          <div class="min-w-0 flex-1 overflow-x-auto">
            <p
              v-if="chartError"
              class="flex items-center text-xs text-muted-foreground"
              :style="{ height: `${HEADER_H}px` }"
              role="status"
            >
              図を表示できませんでした
            </p>
            <GanttChart
              v-else
              :tasks="chart.tasks"
              :rows="chart.rows"
              :deps="chart.deps"
              :view="chart.view"
              :today="today"
              :palette="GANTT_PALETTE"
              :row-height="ROW_H"
              :header-height="HEADER_H"
              row-labels="hidden"
              legend="hidden"
              percent-labels="hidden"
            />
          </div>
        </div>
        <ul v-if="chart.notes.length" class="mt-2 flex flex-col gap-0.5" data-task-blocking-notes>
          <li
            v-for="note in chart.notes"
            :key="note"
            class="text-xs text-[var(--koyori-color-warning)]"
          >
            {{ note }}
          </li>
        </ul>
        <TaskRowHoverCard
          :container="labelsEl"
          :tasks="related"
          :statuses="statuses"
          :project-key="projectKey"
          :tenant-display-id="tenantDisplayId"
        />
      </template>
    </template>
  </div>
</template>

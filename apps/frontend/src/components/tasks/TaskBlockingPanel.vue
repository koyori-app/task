<script setup lang="ts">
import { Ban, Loader2, Plus, RotateCcw, X } from '@lucide/vue';
import { computed, ref, watch } from 'vue';

import TaskParentPicker from '@/components/tasks/TaskParentPicker.vue';
import { Button } from '@koyori-app/ui-vue';
import {
  useTaskBlocking,
  type BlockingRelationType,
  type RelationEntry,
} from '@/composables/useTaskBlocking';
import type { components } from '@/generated/api';
import { taskSeqKey } from '@/lib/task-display';

type TaskResponse = components['schemas']['TaskResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

const props = defineProps<{
  tenantId: string | null | undefined;
  projectId: string | null | undefined;
  /** UUID または PROJECT-42。relations API の解決に使う。 */
  taskId: string;
  /** 自分自身の UUID。候補の除外と自己参照のガードに使う。 */
  taskUuid: string;
  statuses: StatusResponse[];
  projectKey: string;
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

const sections = computed(() => [
  {
    type: 'blocked_by' as const,
    title: 'ブロックされている',
    empty: 'ブロックされているタスクはありません',
    addLabel: 'ブロックされるタスクを追加',
    entries: blocking.blockedBy.value,
  },
  {
    type: 'blocks' as const,
    title: 'ブロックしている',
    empty: 'ブロックしているタスクはありません',
    addLabel: 'ブロックするタスクを追加',
    entries: blocking.blocks.value,
  },
]);

function togglePicker(type: BlockingRelationType) {
  picking.value = picking.value === type ? null : type;
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
</script>

<template>
  <div class="border-t pt-5" data-task-blocking>
    <div class="mb-2 flex items-center gap-2">
      <Ban class="size-4 text-muted-foreground" aria-hidden="true" />
      <h2 class="text-sm font-medium text-muted-foreground">ブロック関係</h2>
      <span
        v-if="!blocking.loading.value && !blocking.error.value && openBlockerCount > 0"
        class="text-xs tabular-nums text-destructive"
        data-task-blocking-open-count
      >
        未完了のブロッカー {{ openBlockerCount }} 件
      </span>
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
      <section v-for="section in sections" :key="section.type" class="mb-3">
        <div class="mb-1 flex items-center justify-between gap-2">
          <h3 class="text-xs text-muted-foreground">{{ section.title }}</h3>
          <Button
            type="button"
            variant="ghost"
            :label="section.addLabel"
            :disabled="blocking.mutating.value"
            @click="togglePicker(section.type)"
          >
            <template #icon><Plus class="size-4" aria-hidden="true" /></template>
          </Button>
        </div>

        <TaskParentPicker
          v-if="picking === section.type"
          :tenant-id="tenantId"
          :project-id="projectId"
          :project-key="projectKey"
          :exclude-task-ids="candidateExclusions"
          :disabled="blocking.mutating.value"
          :input-label="
            section.type === 'blocks' ? 'ブロックするタスクを検索' : 'ブロックされるタスクを検索'
          "
          cancel-label="ブロック相手の選択をやめる"
          @select="add"
          @cancel="picking = null"
        />

        <ul v-if="section.entries.length" class="border-y border-border/60">
          <li
            v-for="entry in section.entries"
            :key="entry.relation_id"
            class="flex items-center border-b border-border/60 last:border-b-0"
            :class="{ 'text-muted-foreground': isDone(entry) }"
          >
            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-2 px-1 py-2 text-left transition-colors hover:bg-muted/40"
              @click="emit('open', entry)"
            >
              <span
                class="size-2.5 shrink-0 rounded-full border"
                :style="
                  statusesById.get(entry.status_id)?.color
                    ? { backgroundColor: statusesById.get(entry.status_id)?.color }
                    : undefined
                "
                aria-hidden="true"
              />
              <span class="shrink-0 font-mono text-xs text-muted-foreground">
                {{ taskSeqKey(projectKey, entry.seq_id) }}
              </span>
              <span class="truncate text-sm">{{ entry.title }}</span>
              <span class="ml-auto shrink-0 text-xs text-muted-foreground">
                {{ statusesById.get(entry.status_id)?.name ?? '—' }}
              </span>
            </button>
            <Button
              type="button"
              variant="ghost"
              class="shrink-0"
              :ariaLabel="`${taskSeqKey(projectKey, entry.seq_id)} との関係を外す`"
              :disabled="blocking.mutating.value"
              @click="remove(entry)"
            >
              <template #icon><X class="size-4" aria-hidden="true" /></template>
            </Button>
          </li>
        </ul>
        <p v-else class="py-1 text-sm text-muted-foreground">{{ section.empty }}</p>
      </section>
    </template>
  </div>
</template>

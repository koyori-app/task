<script setup lang="ts">
import { Check } from '@lucide/vue';
import { Dropdown, Tooltip } from '@koyori-app/ui-vue';
import { computed, useId } from 'vue';

import TaskStatusIcon from '@/components/tasks/TaskStatusIcon.vue';
import type { components } from '@/generated/api';
import { statusFill, statusMenuItems } from '@/lib/task-status-icon';

type StatusOption = components['schemas']['ProjectStatusResponse'];

/**
 * 詳細のステータス。ステータスの色を薄めた背景のボタン（押すとメニュー）と、
 * 離して置いた ✓（既定の完了へ一手で移す）。
 */
const props = defineProps<{
  statuses: StatusOption[];
  statusId: string;
  updating?: boolean;
  /** 完了条件（担当者全員の完了）を満たしておらず、完了へ直接は移せない */
  doneBlocked?: boolean;
  /** まだ自分の分を完了にしていない担当者の数。✓ を押せない理由に出す */
  remainingAssignees?: number;
}>();

const emit = defineEmits<{
  'update:statusId': [value: string];
}>();

const tooltipId = useId();

const current = computed(() => props.statuses.find((status) => status.id === props.statusId));
const fill = computed(() => statusFill(props.statuses, props.statusId));
const items = computed(() =>
  statusMenuItems(props.statuses, props.statusId, {
    disabled: props.updating,
    doneBlocked: props.doneBlocked,
  }),
);

/** ✓ の移動先。既定の完了に印が無ければ並び順で最初の完了へ倒す */
const doneStatus = computed(
  () =>
    props.statuses.find((status) => status.is_done_state && status.is_default_done) ??
    props.statuses.find((status) => status.is_done_state),
);
/** 完了済みなら ✓ は出さない */
const showDone = computed(() => !!doneStatus.value && !current.value?.is_done_state);
const blockedReason = computed(() =>
  props.remainingAssignees
    ? `担当者があと ${props.remainingAssignees} 人残っています`
    : '担当者全員の完了が必要です',
);

function complete() {
  if (!doneStatus.value || props.doneBlocked || props.updating) return;
  emit('update:statusId', doneStatus.value.id);
}
</script>

<template>
  <div class="flex min-h-8 items-center gap-2" data-task-status-button>
    <!--
      文字はステータスの色に載せず、常に濃い色にする（どんな色でも読めるように）。
      Dropdown のトリガーは自分の文字色を持つので、色は名前の要素に直に当てる。
    -->
    <div
      class="inline-flex h-7 max-w-48 min-w-0 items-center rounded-md text-foreground"
      :style="{
        backgroundColor: current
          ? `color-mix(in srgb, ${current.color} 16%, transparent)`
          : 'var(--koyori-color-surface-hover)',
      }"
    >
      <!-- 一覧は根の中に出るので、器で切り取らない（overflow-hidden を付けない） -->
      <Dropdown
        top-layer
        class="min-w-0"
        label="ステータス"
        :disabled="updating"
        :icon="null"
        :items="items"
        :on-select="(id: string) => emit('update:statusId', id)"
      >
        <template #trigger>
          <span class="flex min-w-0 items-center gap-1.5">
            <TaskStatusIcon :status="current" :fill="fill" />
            <span class="truncate text-sm font-medium text-foreground" data-task-status-name>{{
              current?.name ?? '未設定'
            }}</span>
          </span>
        </template>
      </Dropdown>
    </div>

    <template v-if="showDone && doneStatus">
      <!-- 押せない理由を隠さない。disabled にするとフォーカスも理由も届かないので aria-disabled で止める -->
      <Tooltip v-if="doneBlocked" :id="tooltipId" :content="blockedReason">
        <button
          type="button"
          class="inline-flex size-8 shrink-0 cursor-not-allowed items-center justify-center rounded-md border text-muted-foreground opacity-50"
          :aria-label="`${doneStatus.name} にする（完了）`"
          aria-disabled="true"
          data-task-status-done
          @click.prevent
        >
          <Check class="size-4" aria-hidden="true" />
        </button>
      </Tooltip>
      <button
        v-else
        type="button"
        class="inline-flex size-8 shrink-0 items-center justify-center rounded-md border text-muted-foreground transition-colors hover:border-transparent hover:bg-[var(--koyori-color-success)] hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 disabled:opacity-50"
        :aria-label="`${doneStatus.name} にする（完了）`"
        :disabled="updating"
        data-task-status-done
        @click="complete"
      >
        <Check class="size-4" aria-hidden="true" />
      </button>
    </template>
  </div>
</template>

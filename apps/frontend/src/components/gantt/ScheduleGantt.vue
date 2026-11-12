<script setup lang="ts">
import LoadErrorState from '@/components/LoadErrorState.vue';
import { ChevronLeft, ChevronRight, Loader2 } from '@lucide/vue';
import { computed, onErrorCaptured, ref, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import { useMediaQuery } from '@vueuse/core';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';
import { GanttChart, type GanttTask } from '@koyori-app/arc-vue';
import '@koyori-app/arc-vue/style.css';

import { popOverlayParent, pushOverlayParent } from '@/components/tasks/overlay-parent-stack';
import TaskDetailOverlay from '@/components/tasks/TaskDetailOverlay.vue';
import type { TaskRefLink } from '@/components/tasks/TaskDetailPane.vue';
import { Button, Dropdown, type DropdownItem } from '@koyori-app/ui-vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import type { components } from '@/generated/api';
import {
  memberDropdownItems,
  membersAssignedTo,
  toggled,
  UNASSIGNED_MEMBER,
} from '@/lib/calendar/filters';
import { fetchAllSchedule, SCHEDULE_PAGE_LIMIT, SCHEDULE_PATH } from '@/lib/calendar/schedule';
import { addDays, dateInZone } from '@/lib/calendar/timeline';
import {
  applyCalendarUrlState,
  CALENDAR_DAYS,
  parseCalendarUrlState,
  shiftFrom,
} from '@/lib/calendar/url-state';
import { toGanttBarsFor } from '@/lib/gantt/tasks';
import { taskDetailHref, taskSeqKey } from '@/lib/task-display';

type ScheduleTask = components['schemas']['ScheduleTask'];
type TaskResponse = components['schemas']['TaskResponse'];

const props = defineProps<{
  /** 渡せば、そのプロジェクトのタスクだけを出す。省けばテナント全体 */
  projectKey?: string;
}>();

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
  tenantsQuery,
} = useResolvedTenantId(tenantDisplayId);

// プロジェクトで絞る時は、予定の API の project_id で絞る（見えないプロジェクトは API が無視する）
const scoped = computed(() => !!props.projectKey);
const {
  projectId,
  isProjectNotFound: isProjectKeyNotFound,
  isResolving: isProjectKeyResolving,
  isError: isProjectKeyResolveError,
  projectsQuery,
} = useResolvedProjectId(
  // テナント全体の時はプロジェクトの一覧を引かない
  () => (scoped.value ? tenantId.value : null),
  () => props.projectKey ?? '',
);
const isProjectResolving = computed(() => scoped.value && isProjectKeyResolving.value);
const isProjectResolveError = computed(() => scoped.value && isProjectKeyResolveError.value);
const isProjectNotFound = computed(() => scoped.value && isProjectKeyNotFound.value);

// 表に出す期間と選んだ担当者は、カレンダーと同じ決め方と同じ URL の形（`from`・`member`）で持つ
const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
const today = dateInZone(new Date(), timeZone);
const initial = parseCalendarUrlState(pageContext.urlParsed?.searchAll, today);
const from = ref(initial.from);
const memberIds = ref<string[]>(initial.memberIds);
const to = computed(() => addDays(from.value, CALENDAR_DAYS - 1));

watch([from, memberIds], ([nextFrom, nextMembers]) => {
  if (typeof window === 'undefined') return;
  const url = applyCalendarUrlState(
    new URL(window.location.href),
    { from: nextFrom, memberIds: nextMembers, projectIds: [], selected: null },
    today,
  );
  // history を汚さない（カレンダーと同じ）
  window.history.replaceState(window.history.state, '', url);
});

const scheduleQuery = useQuery({
  queryKey: computed(
    () =>
      [
        'get',
        SCHEDULE_PATH,
        {
          params: {
            path: { tenant_id: tenantId.value! },
            query: {
              from: from.value,
              to: to.value,
              timezone: timeZone,
              limit: SCHEDULE_PAGE_LIMIT,
              include_overdue: false,
              ...(scoped.value ? { project_id: [projectId.value!] } : {}),
            },
          },
        },
      ] as const,
  ),
  queryFn: ({ queryKey, signal }) => fetchAllSchedule(queryKey[2].params, signal),
  enabled: computed(() => !!tenantId.value && (!scoped.value || !!projectId.value)),
});

const scheduleTasks = computed(() => scheduleQuery.data.value?.tasks ?? []);
const gantt = computed(() => toGanttBarsFor(scheduleTasks.value, memberIds.value, timeZone));

// 選び肢はカレンダーと同じく、見ている範囲の課題に担当として現れる者。
// カレンダーには担当の無い課題を指す選び肢が無い（担当なしの行が常にある）ので、ガントだけ足す
const memberItems = computed<DropdownItem[]>(() => [
  {
    value: UNASSIGNED_MEMBER,
    label: '担当なし',
    kind: 'checkbox',
    checked: memberIds.value.includes(UNASSIGNED_MEMBER),
  },
  ...memberDropdownItems(
    membersAssignedTo(scheduleTasks.value, scheduleQuery.data.value?.members ?? []),
    memberIds.value,
  ),
]);
const byId = computed(() => new Map(scheduleTasks.value.map((task) => [task.id, task])));

// wasm を読めない時は、arc-vue の onMounted の失敗がここへ上がる。頁ごと落とさず、案内に替える
const chartError = ref(false);
onErrorCaptured(() => {
  chartError.value = true;
  return false;
});
watch(
  () => scheduleTasks.value,
  () => {
    chartError.value = false;
  },
);

// ---- 棒を押した時: カレンダーと同じオーバーレイと戻り先の積み ----
const canInline = useMediaQuery('(min-width: 1024px)');
const selectedTaskId = ref<string | null>(null);
const overlayParentStack = ref<string[]>([]);
const overlayOpenSeqKey = computed(() => (canInline.value ? selectedTaskId.value : null));
const overlayTaskSeqKey = ref<string | null>(null);
watch(
  overlayOpenSeqKey,
  (seqKey) => {
    if (seqKey) overlayTaskSeqKey.value = seqKey;
  },
  { immediate: true },
);
const overlayProjectKey = computed(() => overlayTaskSeqKey.value?.replace(/-\d+$/, '') ?? '');

function onTaskClick(bar: GanttTask) {
  const task: ScheduleTask | undefined = byId.value.get(bar.id);
  if (!task) return;
  if (!canInline.value) {
    void navigate(taskDetailHref(tenantDisplayId.value, task.project.key, task.seq_id));
    return;
  }
  overlayParentStack.value = [];
  selectedTaskId.value = task.seq_key;
}

function openInOverlay(next: string) {
  overlayParentStack.value = pushOverlayParent(
    overlayParentStack.value,
    selectedTaskId.value,
    next,
  );
  selectedTaskId.value = next;
}

function onOverlayOpenChange(open: boolean) {
  if (open) return;
  const { stack, next } = popOverlayParent(overlayParentStack.value);
  overlayParentStack.value = stack;
  selectedTaskId.value = next;
}

function formatRangeDate(date: string) {
  const [year, month, day] = date.split('-');
  return `${year}/${Number(month)}/${Number(day)}`;
}

function retryFailed() {
  if (isTenantResolveError.value) void tenantsQuery.refetch();
  if (isProjectResolveError.value) void projectsQuery.refetch();
  if (scheduleQuery.isError.value) void scheduleQuery.refetch();
}
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-4">
    <div class="flex flex-wrap items-center gap-2">
      <Button variant="tertiary" ariaLabel="前の 2 週間" @click="from = shiftFrom(from, -1)">
        <template #icon><ChevronLeft class="size-4" aria-hidden="true" /></template>
      </Button>
      <Button
        variant="tertiary"
        label="今日"
        @click="from = parseCalendarUrlState({}, today).from"
      />
      <Button variant="tertiary" ariaLabel="次の 2 週間" @click="from = shiftFrom(from, 1)">
        <template #icon><ChevronRight class="size-4" aria-hidden="true" /></template>
      </Button>
      <span class="text-sm font-medium" aria-live="polite">
        {{ formatRangeDate(from) }} 〜 {{ formatRangeDate(to) }}
      </span>

      <div class="ml-auto flex flex-wrap gap-2">
        <Dropdown
          variant="tertiary"
          :label="memberIds.length ? `メンバー（${memberIds.length}）` : 'メンバー'"
          :items="memberItems"
          empty-message="候補がありません"
          :on-select="(id: string) => (memberIds = toggled(memberIds, id))"
        />
        <Button
          v-if="memberIds.length"
          variant="ghost"
          label="絞り込みを解除"
          @click="memberIds = []"
        />
      </div>
    </div>

    <div
      v-if="isTenantResolving || isProjectResolving || scheduleQuery.isLoading.value"
      class="flex justify-center py-8"
    >
      <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" aria-label="読み込み中" />
    </div>

    <LoadErrorState
      v-else-if="isTenantResolveError || isProjectResolveError || scheduleQuery.isError.value"
      message="予定の読み込みに失敗しました"
      @retry="retryFailed"
    />

    <p v-else-if="isTenantNotFound" class="py-8 text-center text-sm text-muted-foreground">
      テナントが見つかりません
    </p>

    <p v-else-if="isProjectNotFound" class="py-8 text-center text-sm text-muted-foreground">
      プロジェクトが見つかりません
    </p>

    <p
      v-else-if="!gantt.bars.length"
      class="py-8 text-center text-sm text-muted-foreground"
      data-testid="gantt-empty"
    >
      この期間に予定のあるタスクはありません
    </p>

    <p
      v-else-if="chartError"
      class="py-8 text-center text-sm text-destructive"
      role="alert"
      data-testid="gantt-chart-error"
    >
      ガントを描けませんでした。ページを読み込み直してください
    </p>

    <template v-else>
      <GanttChart :tasks="gantt.bars" :today="today" @task-click="onTaskClick" />
      <p v-if="gantt.skipped" class="text-xs text-muted-foreground" data-testid="gantt-skipped">
        期間の無い課題 {{ gantt.skipped }} 件は表示していません
      </p>
    </template>

    <TaskDetailOverlay
      v-if="overlayTaskSeqKey"
      :key="overlayTaskSeqKey"
      :open="!!overlayOpenSeqKey"
      :tenant-display-id="tenantDisplayId"
      :project-key="overlayProjectKey"
      :task-id="overlayTaskSeqKey"
      @open-task="(task: TaskResponse) => openInOverlay(taskSeqKey(overlayProjectKey, task.seq_id))"
      @open-task-ref="(ref: TaskRefLink) => openInOverlay(ref.seqKey)"
      @update:open="onOverlayOpenChange"
    />
  </div>
</template>

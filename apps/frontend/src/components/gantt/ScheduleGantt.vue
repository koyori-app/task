<script setup lang="ts">
import LoadErrorState from '@/components/LoadErrorState.vue';
import { ChevronDown, ChevronLeft, ChevronRight, Loader2 } from '@lucide/vue';
import { computed, onErrorCaptured, ref, useTemplateRef, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import { useElementSize, useMediaQuery, useWindowSize } from '@vueuse/core';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';
import { GanttChart, type GanttTask } from '@koyori-app/arc-vue';
import '@koyori-app/arc-vue/style.css';

import { popOverlayParent, pushOverlayParent } from '@/components/tasks/overlay-parent-stack';
import TaskDetailOverlay from '@/components/tasks/TaskDetailOverlay.vue';
import type { TaskRefLink } from '@/components/tasks/TaskDetailPane.vue';
import {
  Avatar,
  Button,
  ButtonGroup,
  Drawer,
  Dropdown,
  Switch,
  type DropdownItem,
} from '@koyori-app/ui-vue';
import { usePreference } from '@/composables/useLocalSettings';
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
import { dateInZone, mondayOf } from '@/lib/calendar/timeline';
import { layoutGantt, monthDay, taskIdOfRow, type GanttListItem } from '@/lib/gantt/layout';
import {
  fetchAllUnscheduled,
  projectScope,
  UNSCHEDULED_PAGE_LIMIT,
  UNSCHEDULED_PATH,
  unscheduledFor,
} from '@/lib/gantt/unscheduled';
import { applyGanttUrlState, parseGanttUrlState } from '@/lib/gantt/url-state';
import {
  chartBackend,
  fitDayWidth,
  shiftView,
  viewTo,
  type GanttGroupBy,
  type GanttScale,
} from '@/lib/gantt/view';
import {
  chartHeight,
  chartProps,
  chartRows,
  GROUP_H,
  HEADER_H,
  rowHeight,
} from '@/lib/gantt/chart';
import { taskDetailHref, taskSeqKey } from '@/lib/task-display';
import { usePreferencesStore } from '@/stores/preferences';

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
const projectFilter = computed(() => projectScope(scoped.value ? projectId.value : null));

// 狭い画面は、題を棒の上に重ねる形にし、日数を減らす（案 A の 375 の形）
const wide = useMediaQuery('(min-width: 1024px)');
const compact = computed(() => !wide.value);

// 表に出す期間と選んだ担当者は、カレンダーと同じ決め方と同じ URL の形（`from`・`member`）で持つ。
// 目盛り・群れ・稲妻線はガントだけの切り替えとして足す
const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
const today = dateInZone(new Date(), timeZone);
const initial = parseGanttUrlState(pageContext.urlParsed?.searchAll, today);
const from = ref(initial.from);
const memberIds = ref<string[]>(initial.memberIds);
const scale = ref<GanttScale>(initial.scale);
const groupBy = ref<GanttGroupBy>(initial.groupBy);
const urlProgressLine = ref<boolean | null>(initial.progressLine);
const to = computed(() => viewTo(from.value, scale.value, compact.value));

// 稲妻線は既定で切れ。URL に `line` があればそれに従い、無ければ利用者が最後に選んだ値を使う
const preferences = usePreferencesStore();
const rememberedProgressLine = usePreference('ganttProgressLine');
const progressLine = computed(() => urlProgressLine.value ?? rememberedProgressLine.value);
function setProgressLine(on: boolean) {
  urlProgressLine.value = on;
  preferences.set('ganttProgressLine', on);
}

watch([from, memberIds, scale, groupBy, urlProgressLine], () => {
  if (typeof window === 'undefined') return;
  const url = applyGanttUrlState(
    new URL(window.location.href),
    {
      from: from.value,
      memberIds: memberIds.value,
      scale: scale.value,
      groupBy: groupBy.value,
      progressLine: urlProgressLine.value,
    },
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
              ...projectFilter.value,
            },
          },
        },
      ] as const,
  ),
  queryFn: ({ queryKey, signal }) => fetchAllSchedule(queryKey[2].params, signal),
  enabled: computed(() => !!tenantId.value && (!scoped.value || !!projectId.value)),
});

// 期間の無い課題は予定の API に出ないので、別の口で引いて数える
const unscheduledQuery = useQuery({
  queryKey: computed(
    () =>
      [
        'get',
        UNSCHEDULED_PATH,
        {
          params: {
            path: { tenant_id: tenantId.value! },
            query: { limit: UNSCHEDULED_PAGE_LIMIT, ...projectFilter.value },
          },
        },
      ] as const,
  ),
  queryFn: ({ queryKey, signal }) => fetchAllUnscheduled(queryKey[2].params, signal),
  enabled: computed(() => !!tenantId.value && (!scoped.value || !!projectId.value)),
});

const scheduleTasks = computed(() => scheduleQuery.data.value?.tasks ?? []);
const members = computed(() => scheduleQuery.data.value?.members ?? []);
const unscheduledTasks = computed(() =>
  unscheduledFor(unscheduledQuery.data.value?.tasks ?? [], memberIds.value),
);

const collapsed = ref(new Set<string>());
function toggleGroup(key: string) {
  const next = new Set(collapsed.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  collapsed.value = next;
}

const layout = computed(() =>
  layoutGantt(scheduleTasks.value, {
    groupBy: groupBy.value,
    collapsed: collapsed.value,
    memberIds: memberIds.value,
    members: members.value,
    timeZone,
  }),
);

// 選び肢はカレンダーと同じく、見ている範囲の課題に担当として現れる者。
// カレンダーには担当の無い課題を指す選び肢が無い（担当なしの行が常にある）ので、ガントだけ足す
const memberItems = computed<DropdownItem[]>(() => [
  {
    value: UNASSIGNED_MEMBER,
    label: '担当なし',
    kind: 'checkbox',
    checked: memberIds.value.includes(UNASSIGNED_MEMBER),
  },
  ...memberDropdownItems(membersAssignedTo(scheduleTasks.value, members.value), memberIds.value),
]);
const groupItems = computed<DropdownItem[]>(() => [
  { value: 'project', label: 'プロジェクト', kind: 'radio', checked: groupBy.value === 'project' },
  { value: 'assignee', label: '担当者', kind: 'radio', checked: groupBy.value === 'assignee' },
]);
const SCALE_LABELS: Record<GanttScale, string> = { day: '日', week: '週', month: '月' };
const memberName = computed(() => new Map(members.value.map((m) => [m.id, m.username])));
const byId = computed(() => new Map(scheduleTasks.value.map((task) => [task.id, task])));

// ---- 図の寸法と色 ----
const rowH = computed(() => rowHeight(compact.value));
const arcRows = computed(() => chartRows(layout.value, compact.value));
const { height: screenHeight } = useWindowSize();
const backend = computed(() =>
  chartBackend({
    compact: compact.value,
    rowCount: arcRows.value.length,
    chartHeight: chartHeight(arcRows.value, compact.value),
    screenHeight: screenHeight.value,
  }),
);

// 図の箱の幅いっぱいに範囲の日を割る
const chartBox = useTemplateRef<HTMLElement>('chartBox');
const { width: chartBoxWidth } = useElementSize(chartBox);
const dayWidth = computed(() => fitDayWidth(chartBoxWidth.value, scale.value, compact.value));

const chart = computed(() =>
  chartProps({
    layout: layout.value,
    today,
    from: from.value,
    to: to.value,
    scale: scale.value,
    pxPerDay: dayWidth.value,
    compact: compact.value,
    progressLine: progressLine.value,
    backend: backend.value,
  }),
);

/** 狭い画面で、図の上に重ねる行の字の位置 */
const overlayRows = computed(() => {
  let y = HEADER_H;
  const out: (
    | { kind: 'group'; key: string; label: string; count: number; collapsed: boolean; y: number }
    | { kind: 'task'; item: GanttListItem; y: number }
  )[] = [];
  for (const group of layout.value.groups) {
    out.push({ kind: 'group', ...group, y });
    y += GROUP_H;
    if (group.collapsed) continue;
    for (const item of group.items) {
      out.push({ kind: 'task', item, y });
      y += rowH.value;
    }
  }
  return out;
});

function assigneeNames(task: ScheduleTask | GanttListItem['task']): string[] {
  return task.assignees.map((a) => memberName.value.get(a.user_id) ?? '不明な利用者');
}

/** 狭い画面の題の下の一行。厳守の期限を棒の中に書けないので、ここに添える */
function compactMeta(item: GanttListItem): string {
  const parts = [item.task.seq_key];
  if (item.task.status.is_done_state) parts.push('完了');
  else if (item.dates.hard) parts.push(`厳守 ${monthDay(item.dates.hard)}`);
  else if (item.dates.soft) parts.push(`目安 ${monthDay(item.dates.soft)}`);
  return parts.join('　');
}

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
const canInline = wide;
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

function openTask(task: { seq_key: string; seq_id: number; project: { key: string } }) {
  if (!canInline.value) {
    void navigate(taskDetailHref(tenantDisplayId.value, task.project.key, task.seq_id));
    return;
  }
  overlayParentStack.value = [];
  selectedTaskId.value = task.seq_key;
}

function onTaskClick(bar: GanttTask) {
  const task = byId.value.get(taskIdOfRow(bar.id));
  if (task) openTask(task);
}

const unscheduledOpen = ref(false);
function openUnscheduled(task: ScheduleTask) {
  unscheduledOpen.value = false;
  openTask(task);
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
  <div class="flex w-full min-w-0 flex-col gap-3">
    <div class="flex flex-wrap items-center gap-2">
      <Button
        variant="tertiary"
        ariaLabel="前の期間"
        @click="from = shiftView(from, scale, compact, -1)"
      >
        <template #icon><ChevronLeft class="size-4" aria-hidden="true" /></template>
      </Button>
      <Button variant="tertiary" label="今日" @click="from = mondayOf(today)" />
      <Button
        variant="tertiary"
        ariaLabel="次の期間"
        @click="from = shiftView(from, scale, compact, 1)"
      >
        <template #icon><ChevronRight class="size-4" aria-hidden="true" /></template>
      </Button>
      <span class="text-sm font-medium" aria-live="polite" data-testid="gantt-range">
        {{ formatRangeDate(from) }} 〜 {{ formatRangeDate(to) }}
      </span>

      <ButtonGroup label="目盛り">
        <Button
          v-for="s in ['day', 'week', 'month'] as const"
          :key="s"
          :variant="scale === s ? 'secondary' : 'tertiary'"
          :ariaPressed="scale === s ? 'true' : 'false'"
          :label="SCALE_LABELS[s]"
          :data-testid="`gantt-scale-${s}`"
          @click="scale = s"
        />
      </ButtonGroup>

      <div class="ml-auto flex flex-wrap items-center gap-2">
        <Dropdown
          variant="tertiary"
          :label="`群れ: ${groupBy === 'project' ? 'プロジェクト' : '担当者'}`"
          :items="groupItems"
          :on-select="(value: string) => (groupBy = value as GanttGroupBy)"
        />
        <Dropdown
          variant="tertiary"
          :label="memberIds.length ? `担当者（${memberIds.length}）` : '担当者'"
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
        <Button
          variant="tertiary"
          :label="`期間なし ${unscheduledTasks.length} 件`"
          :disabled="!unscheduledQuery.isSuccess.value"
          data-testid="gantt-unscheduled"
          @click="unscheduledOpen = true"
        />
      </div>
    </div>

    <!-- 一行の凡例。色は図と同じ token -->
    <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
      <span class="inline-flex items-center gap-1">
        <span
          class="inline-block h-2.5 w-6 rounded-sm"
          style="
            background: linear-gradient(
              to right,
              var(--koyori-color-accent) 40%,
              var(--koyori-color-accent-subtle) 40%
            );
          "
          aria-hidden="true"
        />
        予定開始〜目安の期限（濃い所が進み）
      </span>
      <span class="inline-flex items-center gap-1">
        <span
          class="inline-block size-2 rotate-45"
          style="background: var(--koyori-color-danger)"
          aria-hidden="true"
        />
        厳守の期限
      </span>
      <span class="inline-flex items-center gap-1">
        <span
          class="inline-block h-2.5 w-6 rounded-sm border"
          style="
            background: var(--koyori-color-danger-subtle);
            border-color: var(--koyori-color-danger);
          "
          aria-hidden="true"
        />
        目安が厳守を越えた
      </span>
      <span class="inline-flex items-center gap-1">
        <span
          class="inline-block h-3 w-0.5"
          style="background: var(--koyori-color-text)"
          aria-hidden="true"
        />
        今日
      </span>
      <Switch
        label="稲妻線"
        :checked="progressLine"
        :on-checked-change="setProgressLine"
        data-testid="gantt-progress-line"
      />
      <span v-if="progressLine">今日の時点の進みを結ぶ。左へ尖れば遅れ、右は先行</span>
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
      v-else-if="!layout.groups.length"
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

    <div
      v-else
      class="flex min-w-0 overflow-hidden rounded-md border border-border"
      data-testid="gantt-body"
    >
      <!-- 広い画面: 左の一覧（キー・題・担当・厳守の期限）。行の高さは図と同じ -->
      <div v-if="!compact" class="w-72 shrink-0 border-r border-border" data-testid="gantt-list">
        <div
          class="flex items-end gap-2 px-3 pb-1 text-xs text-muted-foreground"
          :style="{ height: `${HEADER_H}px` }"
        >
          <span class="flex-1">タスク</span>
          <span class="w-12 text-center">担当</span>
          <span class="w-10 text-right">厳守</span>
        </div>
        <template v-for="group in layout.groups" :key="group.key">
          <button
            type="button"
            class="flex w-full items-center gap-1 px-2 text-left text-sm font-semibold"
            :style="{ height: `${GROUP_H}px`, background: 'var(--koyori-color-surface-disabled)' }"
            :aria-expanded="!group.collapsed"
            :data-testid="`gantt-group-${group.key}`"
            @click="toggleGroup(group.key)"
          >
            <ChevronDown
              class="size-4 shrink-0 transition-transform"
              :class="{ '-rotate-90': group.collapsed }"
              aria-hidden="true"
            />
            <span class="min-w-0 flex-1 truncate">{{ group.label }}</span>
            <span class="text-xs font-normal text-muted-foreground">{{ group.count }} 件</span>
          </button>
          <template v-if="!group.collapsed">
            <button
              v-for="item in group.items"
              :key="item.rowId"
              type="button"
              class="flex w-full items-center gap-2 border-t border-border/60 px-3 text-left hover:bg-muted/40"
              :style="{ height: `${rowH}px` }"
              :title="item.task.title"
              @click="openTask(byId.get(item.task.id)!)"
            >
              <span class="min-w-0 flex-1">
                <span class="block font-mono text-[11px] leading-tight text-muted-foreground">
                  {{ item.task.seq_key }}
                </span>
                <span class="block truncate text-sm leading-tight">{{ item.task.title }}</span>
              </span>
              <span class="flex w-12 justify-center" :title="assigneeNames(item.task).join('、')">
                <Avatar
                  v-if="item.task.assignees.length"
                  :name="assigneeNames(item.task)[0]!"
                  :size="20"
                />
              </span>
              <span class="w-10 text-right text-xs text-destructive">
                {{ item.dates.hard ? monthDay(item.dates.hard) : '' }}
              </span>
            </button>
          </template>
        </template>
      </div>

      <div ref="chartBox" class="relative min-w-0 flex-1 overflow-x-auto">
        <GanttChart v-bind="chart" @task-click="onTaskClick" />
        <!-- 狭い画面: 題と群れの見出しを図の行の上に重ねる -->
        <template v-if="compact">
          <template
            v-for="row in overlayRows"
            :key="row.kind === 'group' ? `g:${row.key}` : row.item.rowId"
          >
            <button
              v-if="row.kind === 'group'"
              type="button"
              class="absolute inset-x-0 flex items-center gap-1 px-2 text-left text-sm font-semibold"
              :style="{ top: `${row.y}px`, height: `${GROUP_H}px` }"
              :aria-expanded="!row.collapsed"
              @click="toggleGroup(row.key)"
            >
              <ChevronDown
                class="size-4 shrink-0 transition-transform"
                :class="{ '-rotate-90': row.collapsed }"
                aria-hidden="true"
              />
              <span class="min-w-0 flex-1 truncate">{{ row.label }}</span>
              <span class="text-xs font-normal text-muted-foreground">{{ row.count }} 件</span>
            </button>
            <button
              v-else
              type="button"
              class="absolute inset-x-0 flex items-start gap-2 px-2 pt-1.5 text-left"
              :style="{ top: `${row.y}px`, height: `${rowH - 30}px` }"
              @click="openTask(byId.get(row.item.task.id)!)"
            >
              <Avatar
                v-if="row.item.task.assignees.length"
                :name="assigneeNames(row.item.task)[0]!"
                :size="20"
              />
              <span class="min-w-0 flex-1">
                <span class="block truncate text-sm leading-tight">{{ row.item.task.title }}</span>
                <span class="block truncate text-[11px] leading-tight text-muted-foreground">
                  {{ compactMeta(row.item) }}
                </span>
              </span>
            </button>
          </template>
        </template>
      </div>
    </div>

    <Drawer
      :open="unscheduledOpen"
      label="期間の無いタスク"
      placement="right"
      :on-close="() => (unscheduledOpen = false)"
    >
      <div class="flex flex-col gap-2 p-4" data-testid="gantt-unscheduled-list">
        <h2 class="text-base font-semibold">期間の無いタスク {{ unscheduledTasks.length }} 件</h2>
        <p class="text-xs text-muted-foreground">
          開始予定日も期限も無いので、図には出ません。開いて日付を入れると図に出ます。
        </p>
        <button
          v-for="task in unscheduledTasks"
          :key="task.id"
          type="button"
          class="rounded-md px-2 py-1.5 text-left hover:bg-muted/40"
          @click="openUnscheduled(task)"
        >
          <span class="block font-mono text-[11px] text-muted-foreground">{{ task.seq_key }}</span>
          <span class="block truncate text-sm">{{ task.title }}</span>
        </button>
      </div>
    </Drawer>

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

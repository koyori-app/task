<script setup lang="ts">
import { ChevronLeft, ChevronRight, Loader2 } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import { useMediaQuery } from '@vueuse/core';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';

import CalendarTimeline from '@/components/calendar/CalendarTimeline.vue';
import { popOverlayParent, pushOverlayParent } from '@/components/tasks/overlay-parent-stack';
import TaskDetailOverlay from '@/components/tasks/TaskDetailOverlay.vue';
import type { TaskRefLink } from '@/components/tasks/TaskDetailPane.vue';
import { Button, Dropdown, type DropdownItem } from '@koyori-app/ui-vue';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';
import { dailyLoadByMember } from '@/lib/calendar/load';
import { addDays, clipSpan, dateInZone, taskSpan } from '@/lib/calendar/timeline';
import {
  applyCalendarUrlState,
  CALENDAR_DAYS,
  parseCalendarUrlState,
  shiftFrom,
} from '@/lib/calendar/url-state';
import { taskDetailHref, taskSeqKey } from '@/lib/task-display';

type ScheduleResponse = components['schemas']['ScheduleResponse'];
type Member = components['schemas']['UserSummary'];
type ScheduleTask = components['schemas']['ScheduleTask'];
type TaskResponse = components['schemas']['TaskResponse'];

const SCHEDULE_PATH = '/v1/tenants/{tenant_id}/schedule' as const;
/** API の上限。14 日なら実用上 1〜2 ページで済む */
const SCHEDULE_PAGE_LIMIT = 500;

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);

const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
const today = dateInZone(new Date(), timeZone);

const initial = parseCalendarUrlState(pageContext.urlParsed?.searchAll, today);
const from = ref(initial.from);
const memberIds = ref<string[]>(initial.memberIds);
const projectIds = ref<string[]>(initial.projectIds);
const to = computed(() => addDays(from.value, CALENDAR_DAYS - 1));
// オーバーレイで開いているタスク（seq key）
const selectedTaskId = ref<string | null>(initial.selected);

watch(
  [from, memberIds, projectIds, selectedTaskId],
  ([nextFrom, nextMembers, nextProjects, selected]) => {
    if (typeof window === 'undefined') return;
    const url = applyCalendarUrlState(
      new URL(window.location.href),
      { from: nextFrom, memberIds: nextMembers, projectIds: nextProjects, selected },
      today,
    );
    // history を汚さない（タスク一覧と同じ）
    window.history.replaceState(window.history.state, '', url);
  },
);

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
              // 締切超過の残工数は今日に積むので、今日が見えている間は期間より前に締切を過ぎたものも取る
              include_overdue: from.value <= today && today <= to.value,
            },
          },
        },
      ] as const,
  ),
  // next_cursor を辿って全件にする。スプリント等は全ページ同じなので先頭ページのものを使う
  queryFn: async ({ queryKey, signal }): Promise<ScheduleResponse> => {
    const { path, query } = queryKey[2].params;
    const fetchPage = async (cursor?: string) => {
      const { data, error } = await fetchClient.GET(SCHEDULE_PATH, {
        params: { path, query: { ...query, cursor } },
        signal,
      });
      if (error) throw error;
      return data;
    };
    const first = await fetchPage();
    const tasks = [...first.tasks];
    const members = new Map<string, Member>(first.members.map((m) => [m.id, m]));
    for (let cursor = first.next_cursor; cursor;) {
      const page = await fetchPage(cursor);
      tasks.push(...page.tasks);
      for (const member of page.members) members.set(member.id, member);
      cursor = page.next_cursor;
    }
    return { ...first, tasks, members: [...members.values()], next_cursor: null };
  },
  enabled: computed(() => !!tenantId.value),
});

// include_overdue で取った期間外のタスクは山に積むものだけ残す。帯には出ないので、
// 残すと帯も山も無いメンバーの行や絞り込み候補が増える
const schedule = computed(() => {
  const data = scheduleQuery.data.value;
  if (!data) return null;
  const tasks = data.tasks.filter((task) => {
    const span = taskSpan(task, timeZone);
    if (span && clipSpan(span, from.value, CALENDAR_DAYS)) return true;
    return dailyLoadByMember([task], timeZone, today).size > 0;
  });
  const assigned = new Set(tasks.flatMap((task) => task.assignees.map((a) => a.user_id)));
  return { ...data, tasks, members: data.members.filter((member) => assigned.has(member.id)) };
});

const memberOptions = computed(() => schedule.value?.members ?? []);
const projectOptions = computed(() => {
  const projects = new Map<string, { id: string; key: string; name: string }>();
  for (const task of schedule.value?.tasks ?? []) projects.set(task.project.id, task.project);
  return [...projects.values()];
});

const filtered = computed(() => {
  const data = schedule.value;
  if (!data) return null;
  const byProject = (projectId: string) =>
    !projectIds.value.length || projectIds.value.includes(projectId);
  const byMember = (id: string) => !memberIds.value.length || memberIds.value.includes(id);
  return {
    tasks: data.tasks.filter(
      (task) =>
        byProject(task.project.id) &&
        (!memberIds.value.length || task.assignees.some((a) => byMember(a.user_id))),
    ),
    members: data.members.filter((member) => byMember(member.id)),
    sprints: data.sprints.filter((sprint) => byProject(sprint.project_id)),
    milestones: data.milestones.filter((milestone) => byProject(milestone.project_id)),
  };
});

// ---- タスクの詳細: タスク一覧の List 表示と同じオーバーレイと戻り先の積み ----
// 狭い画面はタスク一覧と同じく詳細ページへ送る（オーバーレイの 3 カラムが収まらない）
const canInline = useMediaQuery('(min-width: 1024px)');
const overlayParentStack = ref<string[]>([]);
// useMediaQuery はマウント前は false なので、URL から復元した選択もマウント後に開く（SSR と揃う）
const overlayOpenSeqKey = computed(() => (canInline.value ? selectedTaskId.value : null));
// 閉じても最後に開いたタスクを持ち続け、オーバーレイを外さずに open だけを false にする
// （v-if で外すと <dialog> が close() を通らず、開く前の要素へフォーカスが戻らない。
// 閉じている間のペインは TaskDetailOverlay が v-if で外す）
const overlayTaskSeqKey = ref<string | null>(null);
watch(
  overlayOpenSeqKey,
  (seqKey) => {
    if (seqKey) overlayTaskSeqKey.value = seqKey;
  },
  { immediate: true },
);
// カレンダーはテナント横断なので、プロジェクトは seq key（KEY-N）の KEY から取る
const overlayProjectKey = computed(() => overlayTaskSeqKey.value?.replace(/-\d+$/, '') ?? '');

function openTask(task: ScheduleTask) {
  if (!canInline.value) {
    void navigate(taskDetailHref(tenantDisplayId.value, task.project.key, task.seq_id));
    return;
  }
  overlayParentStack.value = [];
  selectedTaskId.value = task.seq_key;
}

/** オーバーレイの中から子タスク・説明の KEY-N を開く。戻れるように今のタスクを積む */
function openInOverlay(next: string) {
  overlayParentStack.value = pushOverlayParent(
    overlayParentStack.value,
    selectedTaskId.value,
    next,
  );
  selectedTaskId.value = next;
}

/** Esc・外側クリック・× で閉じる。子から開いていたら親へ 1 段戻る */
function onOverlayOpenChange(open: boolean) {
  if (open) return;
  const { stack, next } = popOverlayParent(overlayParentStack.value);
  overlayParentStack.value = stack;
  selectedTaskId.value = next;
}

function toggled(list: string[], id: string) {
  return list.includes(id) ? list.filter((v) => v !== id) : [...list, id];
}

function formatRangeDate(date: string) {
  const [year, month, day] = date.split('-');
  return `${year}/${Number(month)}/${Number(day)}`;
}

/** 絞り込みは checkbox。選んでもメニューは閉じないので、続けて複数を選べる */
const memberItems = computed<DropdownItem[]>(() =>
  memberOptions.value.map((member) => ({
    value: member.id,
    label: member.username,
    kind: 'checkbox',
    checked: memberIds.value.includes(member.id),
  })),
);
const projectItems = computed<DropdownItem[]>(() =>
  projectOptions.value.map((project) => ({
    value: project.id,
    label: `${project.key} ${project.name}`,
    kind: 'checkbox',
    checked: projectIds.value.includes(project.id),
  })),
);
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">カレンダー</h1>
      <p class="text-sm text-muted-foreground">メンバーごとのタスクの予定（2 週間）</p>
    </div>

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
        <Dropdown
          variant="tertiary"
          :label="projectIds.length ? `プロジェクト（${projectIds.length}）` : 'プロジェクト'"
          :items="projectItems"
          empty-message="候補がありません"
          :on-select="(id: string) => (projectIds = toggled(projectIds, id))"
        />

        <Button
          v-if="memberIds.length || projectIds.length"
          variant="ghost"
          label="絞り込みを解除"
          @click="
            memberIds = [];
            projectIds = [];
          "
        />
      </div>
    </div>

    <div v-if="isTenantResolving || scheduleQuery.isLoading.value" class="flex justify-center py-8">
      <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" aria-label="読み込み中" />
    </div>

    <p
      v-else-if="isTenantResolveError || scheduleQuery.isError.value"
      class="py-8 text-center text-sm text-destructive"
    >
      予定の読み込みに失敗しました
    </p>

    <p v-else-if="isTenantNotFound" class="py-8 text-center text-sm text-muted-foreground">
      テナントが見つかりません
    </p>

    <CalendarTimeline
      v-else-if="filtered"
      :from="from"
      :days="CALENDAR_DAYS"
      :today="today"
      :time-zone="timeZone"
      :tenant-slug="tenantDisplayId"
      :tenant-id="tenantId"
      :tasks="filtered.tasks"
      :load-tasks="schedule?.tasks"
      :members="filtered.members"
      :sprints="filtered.sprints"
      :milestones="filtered.milestones"
      :show-unassigned="!memberIds.length"
      @open-task="openTask"
    />

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

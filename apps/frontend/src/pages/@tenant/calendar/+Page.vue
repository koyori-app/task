<script setup lang="ts">
import { ChevronDown, ChevronLeft, ChevronRight, Loader2 } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import { usePageContext } from 'vike-vue/usePageContext';

import CalendarTimeline from '@/components/calendar/CalendarTimeline.vue';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';
import { addDays, dateInZone } from '@/lib/calendar/timeline';
import {
  applyCalendarUrlState,
  CALENDAR_DAYS,
  parseCalendarUrlState,
  shiftFrom,
} from '@/lib/calendar/url-state';

type ScheduleResponse = components['schemas']['ScheduleResponse'];
type Member = components['schemas']['UserSummary'];

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

watch([from, memberIds, projectIds], ([nextFrom, nextMembers, nextProjects]) => {
  if (typeof window === 'undefined') return;
  const url = applyCalendarUrlState(
    new URL(window.location.href),
    { from: nextFrom, memberIds: nextMembers, projectIds: nextProjects },
    today,
  );
  // history を汚さない（タスク一覧と同じ）
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

const memberOptions = computed(() => scheduleQuery.data.value?.members ?? []);
const projectOptions = computed(() => {
  const projects = new Map<string, { id: string; key: string; name: string }>();
  for (const task of scheduleQuery.data.value?.tasks ?? [])
    projects.set(task.project.id, task.project);
  return [...projects.values()];
});

const filtered = computed(() => {
  const data = scheduleQuery.data.value;
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

function toggled(list: string[], id: string) {
  return list.includes(id) ? list.filter((v) => v !== id) : [...list, id];
}

function formatRangeDate(date: string) {
  const [year, month, day] = date.split('-');
  return `${year}/${Number(month)}/${Number(day)}`;
}
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">カレンダー</h1>
      <p class="text-sm text-muted-foreground">メンバーごとのタスクの予定（2 週間）</p>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <Button
        variant="outline"
        size="icon"
        aria-label="前の 2 週間"
        @click="from = shiftFrom(from, -1)"
      >
        <ChevronLeft class="size-4" aria-hidden="true" />
      </Button>
      <Button variant="outline" size="sm" @click="from = parseCalendarUrlState({}, today).from">
        今日
      </Button>
      <Button
        variant="outline"
        size="icon"
        aria-label="次の 2 週間"
        @click="from = shiftFrom(from, 1)"
      >
        <ChevronRight class="size-4" aria-hidden="true" />
      </Button>
      <span class="text-sm font-medium" aria-live="polite">
        {{ formatRangeDate(from) }} 〜 {{ formatRangeDate(to) }}
      </span>

      <div class="ml-auto flex flex-wrap gap-2">
        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="outline" size="sm">
              メンバー{{ memberIds.length ? `（${memberIds.length}）` : '' }}
              <ChevronDown class="size-4 opacity-50" aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" class="min-w-48">
            <p v-if="!memberOptions.length" class="px-2 py-1.5 text-sm text-muted-foreground">
              候補がありません
            </p>
            <DropdownMenuCheckboxItem
              v-for="member in memberOptions"
              :key="member.id"
              :model-value="memberIds.includes(member.id)"
              @select="(event: Event) => event.preventDefault()"
              @update:model-value="memberIds = toggled(memberIds, member.id)"
            >
              {{ member.username }}
            </DropdownMenuCheckboxItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="outline" size="sm">
              プロジェクト{{ projectIds.length ? `（${projectIds.length}）` : '' }}
              <ChevronDown class="size-4 opacity-50" aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" class="min-w-48">
            <p v-if="!projectOptions.length" class="px-2 py-1.5 text-sm text-muted-foreground">
              候補がありません
            </p>
            <DropdownMenuCheckboxItem
              v-for="project in projectOptions"
              :key="project.id"
              :model-value="projectIds.includes(project.id)"
              @select="(event: Event) => event.preventDefault()"
              @update:model-value="projectIds = toggled(projectIds, project.id)"
            >
              <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{{ project.key }}</span>
              {{ project.name }}
            </DropdownMenuCheckboxItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <Button
          v-if="memberIds.length || projectIds.length"
          variant="ghost"
          size="sm"
          @click="
            memberIds = [];
            projectIds = [];
          "
        >
          絞り込みを解除
        </Button>
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
      :tasks="filtered.tasks"
      :members="filtered.members"
      :sprints="filtered.sprints"
      :milestones="filtered.milestones"
      :show-unassigned="!memberIds.length"
    />
  </div>
</template>

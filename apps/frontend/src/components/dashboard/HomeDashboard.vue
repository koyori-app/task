<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/vue-query';
import {
  CalendarDays,
  Check,
  CheckCheck,
  ChevronRight,
  Circle,
  Clock3,
  ListTodo,
  Loader2,
  Plus,
} from '@lucide/vue';
import type { components } from '@/generated/api';
import type { TenantUuid } from '@/lib/api-ids';
import { fetchClient, useMeQuery } from '@/lib/api-vue-query';
import { taskDetailHref, taskListHref, PRIORITY_CONFIG } from '@/lib/task-display';
import { activityActor, activityText, relativeTime } from '@/lib/task-activity';
import { useNow } from '@/composables/useNow';
import { useHydrated } from '@/composables/useHydrated';
import { Button } from '@/components/ui/button';
import DashboardCreateTask from './DashboardCreateTask.vue';

const props = defineProps<{ tenantId: TenantUuid; tenantSlug: string }>();
const DASHBOARD_PATH = '/v1/tenants/{tenant_id}/users/me/dashboard' as const;
const MY_TASKS_PATH = '/v1/tenants/{tenant_id}/users/me/tasks' as const;
const TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
type DashboardTask = components['schemas']['DashboardTask'];
type Filter = 'today' | 'week' | 'overdue' | 'all';
const tabs: { key: Filter; label: string; count: 'today' | 'week' | 'overdue' | 'open' }[] = [
  { key: 'today', label: '今日', count: 'today' },
  { key: 'week', label: '今週', count: 'week' },
  { key: 'overdue', label: '期限超過', count: 'overdue' },
  { key: 'all', label: 'すべて', count: 'open' },
];
const filter = ref<Filter>('today');
const offset = ref(0);
const pageSize = 5;
const createOpen = ref(false);
const pending = ref<string[]>([]);
const mutationError = ref('');
const now = useNow();
const hydrated = useHydrated();
const me = useMeQuery();
const queryClient = useQueryClient();
const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
const query = useQuery({
  queryKey: computed(
    () =>
      [
        'get',
        DASHBOARD_PATH,
        {
          params: {
            path: { tenant_id: props.tenantId },
            query: { filter: filter.value, timezone, limit: pageSize, offset: offset.value },
          },
        },
      ] as const,
  ),
  queryFn: async ({ queryKey, signal }) => {
    const { data, error } = await fetchClient.GET(DASHBOARD_PATH, {
      params: queryKey[2].params,
      signal,
    });
    if (error) throw error;
    return data;
  },
  // The page keys this component by tenant ID; retain the overview only across its filters/pages.
  placeholderData: keepPreviousData,
  enabled: hydrated,
  refetchInterval: 60_000,
});
const data = computed(() => query.data.value);
const projects = computed(() => data.value?.projects.filter((p) => !p.project.is_personal) ?? []);
const dateLabel = computed(() =>
  now.value.toLocaleDateString('ja-JP', { month: 'long', day: 'numeric', weekday: 'long' }),
);
const today = computed(() =>
  new Intl.DateTimeFormat('en-CA', {
    timeZone: timezone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(now.value),
);
const weekLabel = computed(() => {
  const days = data.value?.days;
  return days?.length
    ? `${days[0]!.date.slice(5).replace('-', '.')} — ${days[6]!.date.slice(5).replace('-', '.')}`
    : '';
});
const maxDailyCount = computed(() =>
  Math.max(1, ...(data.value?.days.map((day) => day.count) ?? [])),
);
const stats = computed(() => [
  {
    label: '今日が期限',
    value: data.value?.counts.today,
    note: '今日のうちに進めたいタスク',
    icon: CalendarDays,
    filter: 'today' as const,
    tone: '',
  },
  {
    label: '期限超過',
    value: data.value?.counts.overdue,
    note: '期限を確認しましょう',
    icon: Clock3,
    filter: 'overdue' as const,
    tone: 'text-red-600 dark:text-red-400',
  },
  {
    label: '未完了',
    value: data.value?.counts.open,
    note: '自分に割り当てられたタスク',
    icon: ListTodo,
    filter: 'all' as const,
    tone: '',
  },
  {
    label: '今週の完了',
    value: data.value?.counts.completed_week,
    note: '月曜日から日曜日まで',
    icon: CheckCheck,
    filter: null,
    tone: 'text-emerald-700 dark:text-emerald-400',
  },
]);
watch(
  filter,
  () => {
    offset.value = 0;
    mutationError.value = '';
  },
  { flush: 'sync' },
);
// A completion can remove the last row of a later page.
watch(
  () => data.value?.total,
  (total) => {
    if (
      !query.isPlaceholderData.value &&
      total !== undefined &&
      offset.value > 0 &&
      offset.value >= total
    )
      offset.value = Math.max(0, Math.ceil(total / pageSize - 1) * pageSize);
  },
);
function deadline(task: DashboardTask['task']) {
  const date = (task.soft_deadline ?? task.hard_deadline)?.slice(0, 10);
  return {
    text: date ? (date === today.value ? '今日' : date.replaceAll('-', '/')) : '期限なし',
    overdue: !!date && date < today.value,
  };
}
async function refresh() {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['get', DASHBOARD_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', MY_TASKS_PATH] }),
    queryClient.invalidateQueries({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks'],
    }),
    queryClient.invalidateQueries({ queryKey: ['get', TASK_PATH] }),
    queryClient.invalidateQueries({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search'],
    }),
    queryClient.invalidateQueries({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/relations'],
    }),
    queryClient.invalidateQueries({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/activities'],
    }),
  ]);
}
async function complete(item: DashboardTask) {
  if (!item.done_status_id || pending.value.includes(item.task.id)) return;
  const id = item.task.id;
  pending.value.push(id);
  mutationError.value = '';
  try {
    const { error } = await fetchClient.PUT(TASK_PATH, {
      params: { path: { tenant_id: props.tenantId, project_id: item.task.project.id, id } },
      body: { status_id: item.done_status_id },
    });
    if (error) throw error;
    await refresh();
  } catch {
    mutationError.value =
      '完了にできませんでした。権限や通信状況を確認して、もう一度お試しください。';
  } finally {
    pending.value = pending.value.filter((taskId) => taskId !== id);
  }
}
</script>

<template>
  <div class="mx-auto w-full max-w-7xl px-1 pb-6 pt-2 sm:px-5 lg:px-7">
    <div class="mb-7 flex items-center justify-between gap-4">
      <div>
        <p class="mb-2 text-xs text-muted-foreground">{{ dateLabel }}</p>
        <h1 class="text-xl font-semibold tracking-tight sm:text-2xl">
          おかえりなさい、<span class="max-sm:block"
            >{{ me.data.value?.username ?? 'ユーザー' }}さん</span
          >
        </h1>
        <p class="mt-2 text-xs text-muted-foreground sm:text-sm">
          今日のタスクと、チームの動きをまとめて。
        </p>
      </div>
      <Button
        class="shrink-0 gap-1.5 max-sm:px-2 max-sm:text-xs"
        :disabled="!data?.projects.length || query.isError.value"
        @click="createOpen = true"
        ><Plus class="size-4" />タスクを作成</Button
      >
    </div>

    <div
      v-if="query.isError.value"
      role="alert"
      class="mb-6 flex flex-wrap items-center gap-3 rounded-lg border border-destructive/30 p-4"
    >
      <p class="text-sm text-destructive">ダッシュボードを取得できませんでした。</p>
      <Button
        variant="outline"
        size="sm"
        :disabled="query.isFetching.value"
        @click="query.refetch()"
        >再試行</Button
      >
    </div>
    <div
      v-if="!data && !query.isError.value"
      role="status"
      class="py-20 text-center text-sm text-muted-foreground"
    >
      <Loader2 class="mx-auto mb-3 size-5 animate-spin" />読み込み中…
    </div>
    <template v-if="data">
      <section class="mb-8 grid grid-cols-2 gap-3 lg:grid-cols-4" aria-label="自分のタスクの概要">
        <component
          :is="stat.filter ? 'button' : 'div'"
          v-for="stat in stats"
          :key="stat.label"
          class="rounded-lg border p-4 text-left sm:p-5"
          :class="stat.filter && 'hover:bg-muted/40'"
          @click="stat.filter && (filter = stat.filter)"
        >
          <span class="flex items-center justify-between text-xs text-muted-foreground"
            >{{ stat.label }}<component :is="stat.icon" class="size-4"
          /></span>
          <span class="mt-2 flex items-baseline gap-2"
            ><strong class="text-3xl font-medium tabular-nums" :class="stat.tone">{{
              stat.value
            }}</strong
            ><span class="text-xs text-muted-foreground">件</span></span
          >
          <span class="mt-1 block text-[11px] text-muted-foreground">{{ stat.note }}</span>
        </component>
      </section>
      <div class="grid gap-8 xl:grid-cols-[minmax(0,1fr)_17rem]">
        <section class="min-w-0" aria-labelledby="dashboard-tasks-heading">
          <div class="mb-4 flex items-center justify-between">
            <h2 id="dashboard-tasks-heading" class="text-sm font-semibold">自分のタスク</h2>
            <a
              :href="`/${tenantSlug}/my-tasks`"
              class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
              >My Tasks を開く<ChevronRight class="size-3.5"
            /></a>
          </div>
          <div class="flex gap-5 overflow-x-auto border-b" aria-label="タスクの絞り込み">
            <button
              v-for="tab in tabs"
              :key="tab.key"
              :aria-pressed="filter === tab.key"
              class="flex shrink-0 items-center gap-1.5 border-b-2 pb-3 text-xs"
              :class="
                filter === tab.key
                  ? 'border-foreground font-medium'
                  : 'border-transparent text-muted-foreground'
              "
              @click="filter = tab.key"
            >
              {{ tab.label
              }}<span class="rounded bg-muted px-1.5 text-[10px] tabular-nums">{{
                data.counts[tab.count]
              }}</span>
            </button>
          </div>
          <p v-if="mutationError" role="alert" class="mt-3 text-xs text-destructive">
            {{ mutationError }}
          </p>
          <p
            v-if="query.isPlaceholderData.value"
            role="status"
            class="min-h-56 py-16 text-center text-sm text-muted-foreground"
          >
            タスクを読み込み中…
          </p>
          <ul v-else class="min-h-56">
            <li
              v-for="item in data.tasks"
              :key="item.task.id"
              class="flex items-center gap-3 border-b py-5"
            >
              <button
                class="group shrink-0 rounded-full p-0.5 text-muted-foreground hover:text-emerald-700 disabled:opacity-40"
                :disabled="!item.done_status_id || pending.includes(item.task.id)"
                :aria-label="`${item.task.title}を完了にする`"
                :title="
                  item.done_status_id
                    ? '完了にする'
                    : 'プロジェクト設定で既定の完了ステータスを選んでください'
                "
                @click="complete(item)"
              >
                <Loader2 v-if="pending.includes(item.task.id)" class="size-4 animate-spin" />
                <template v-else
                  ><Circle class="size-4 group-hover:hidden" /><Check
                    class="hidden size-4 group-hover:block"
                /></template>
              </button>
              <div class="min-w-0 flex-1">
                <a
                  :href="taskDetailHref(tenantSlug, item.task.project.key, item.task.seq_id)"
                  class="block break-words text-sm hover:underline"
                  >{{ item.task.title }}</a
                >
                <div
                  class="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground"
                >
                  <span>{{ item.task.seq_key }}</span
                  ><span aria-hidden="true">·</span><span>{{ item.task.project.name }}</span
                  ><span aria-hidden="true">·</span
                  ><span class="flex items-center gap-1"
                    ><span
                      class="size-1.5 rounded-full"
                      :style="{ backgroundColor: item.task.status.color }"
                    />{{ item.task.status.name }}</span
                  >
                </div>
              </div>
              <div class="shrink-0 text-right text-[11px] text-muted-foreground">
                <span
                  v-if="['High', 'Critical', 'CriticalFire'].includes(item.task.priority)"
                  class="mb-1 block rounded bg-amber-50 px-1.5 py-0.5 text-[10px] text-amber-800 dark:bg-amber-950 dark:text-amber-300"
                  >優先度 {{ PRIORITY_CONFIG[item.task.priority].label }}</span
                >
                <span :class="deadline(item.task).overdue && 'text-red-600 dark:text-red-400'">{{
                  deadline(item.task).text
                }}</span>
              </div>
            </li>
            <li v-if="!data.tasks.length" class="py-16 text-center text-sm text-muted-foreground">
              {{
                filter === 'today'
                  ? '今日が期限の未完了タスクはありません。'
                  : 'この条件の未完了タスクはありません。'
              }}
            </li>
          </ul>
          <div class="mt-4 flex flex-wrap items-center justify-between gap-3">
            <Button
              variant="ghost"
              size="sm"
              class="h-auto gap-1 p-0 text-xs text-muted-foreground"
              :disabled="!data.projects.length"
              @click="createOpen = true"
              ><Plus class="size-3.5" />タスクを追加</Button
            >
            <div
              class="flex items-center gap-3 text-[11px] text-muted-foreground"
              aria-live="polite"
            >
              <span
                >{{ data.total ? offset + 1 : 0 }}–{{
                  Math.min(offset + data.tasks.length, data.total)
                }}
                / {{ data.total }}件</span
              >
              <Button
                v-if="offset > 0"
                :disabled="query.isPlaceholderData.value"
                variant="outline"
                size="sm"
                @click="offset = Math.max(0, offset - pageSize)"
                >前へ</Button
              >
              <Button
                v-if="offset + pageSize < data.total"
                :disabled="query.isPlaceholderData.value"
                variant="outline"
                size="sm"
                @click="offset += pageSize"
                >次へ</Button
              >
            </div>
          </div>
          <p class="mt-3 text-[11px] text-muted-foreground">
            期限が近い順。期限が未設定の場合は最終期限を使います。
          </p>
        </section>
        <div class="grid gap-7 md:grid-cols-2 xl:grid-cols-1">
          <section
            class="rounded-lg border border-emerald-900/10 bg-emerald-900/[0.025] p-5 dark:bg-emerald-200/5"
            aria-labelledby="dashboard-week-heading"
          >
            <div class="flex items-center justify-between gap-2">
              <h2 id="dashboard-week-heading" class="text-xs font-semibold">今週の積み重ね</h2>
              <span class="text-[10px] text-muted-foreground">{{ weekLabel }}</span>
            </div>
            <p class="mt-3">
              <strong class="text-3xl font-medium tabular-nums">{{
                data.counts.completed_week
              }}</strong
              ><span class="ml-2 text-[11px] text-muted-foreground">タスク完了</span>
            </p>
            <div class="mt-4 grid h-20 grid-cols-7 items-end gap-2" aria-label="曜日別の完了数">
              <div
                v-for="(day, index) in data.days"
                :key="day.date"
                class="flex h-full flex-col items-center justify-end gap-1 text-[10px] text-muted-foreground"
                :aria-label="`${day.date}: ${day.count}件完了`"
                :title="`${day.date}: ${day.count}件完了`"
              >
                <span class="text-[9px] tabular-nums">{{ day.count || '' }}</span
                ><span
                  class="w-4 rounded-t-sm"
                  :class="
                    day.date === today
                      ? 'bg-emerald-800/65 dark:bg-emerald-400/70'
                      : 'bg-emerald-900/20 dark:bg-emerald-400/20'
                  "
                  :style="{ height: `${Math.max(3, (day.count / maxDailyCount) * 42)}px` }"
                /><span>{{ ['月', '火', '水', '木', '金', '土', '日'][index] }}</span>
              </div>
            </div>
          </section>
          <section aria-labelledby="dashboard-updates-heading">
            <div class="mb-4 flex items-center justify-between">
              <h2 id="dashboard-updates-heading" class="text-xs font-semibold">最近の更新</h2>
              <span class="text-[10px] text-muted-foreground">参加プロジェクト</span>
            </div>
            <ul class="space-y-4">
              <li
                v-for="item in data.activities"
                :key="item.activity.id"
                class="flex items-start gap-2.5"
              >
                <span
                  aria-hidden="true"
                  class="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-[10px] text-muted-foreground"
                  >{{ Array.from(activityActor(item.activity))[0] }}</span
                >
                <div class="min-w-0 text-[11px] leading-relaxed text-muted-foreground">
                  <p class="break-words">
                    {{ activityActor(item.activity) }}が{{ activityText(item.activity) }}
                  </p>
                  <a
                    :href="taskDetailHref(tenantSlug, item.project_key, item.task_seq_id)"
                    class="mt-0.5 block break-words font-medium text-foreground hover:underline"
                    >{{ item.task_title }}</a
                  >
                  <p class="mt-1 text-[10px]">
                    {{ item.project_name }} · {{ relativeTime(item.activity.created_at, now) }}
                  </p>
                </div>
              </li>
            </ul>
            <p v-if="!data.activities.length" class="text-xs text-muted-foreground">
              まだ更新はありません。
            </p>
          </section>
        </div>
      </div>
      <section class="mt-8 border-t pt-6" aria-labelledby="dashboard-projects-heading">
        <div class="mb-4 flex flex-wrap items-center justify-between gap-2">
          <h2 id="dashboard-projects-heading" class="text-sm font-semibold">
            プロジェクト<span class="ml-3 text-[11px] font-normal text-muted-foreground"
              >チーム全体の進捗</span
            >
          </h2>
          <span class="text-xs text-muted-foreground">{{ projects.length }}プロジェクト</span>
        </div>
        <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          <a
            v-for="item in projects"
            :key="item.project.id"
            :href="taskListHref(tenantSlug, item.project.key)"
            class="min-w-0 rounded-lg border p-4 hover:bg-muted/30"
          >
            <div class="flex items-center gap-2.5">
              <span
                class="flex size-7 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted text-xs text-muted-foreground"
                ><img
                  v-if="item.project.icon_url"
                  :src="item.project.icon_url"
                  class="size-full object-cover"
                  alt=""
                /><template v-else>{{
                  item.project.icon_emoji || Array.from(item.project.name)[0]
                }}</template></span
              ><span class="min-w-0 flex-1 truncate text-sm font-medium">{{
                item.project.name
              }}</span
              ><ChevronRight class="size-3.5 text-muted-foreground" />
            </div>
            <p class="my-3 line-clamp-1 min-h-4 text-xs text-muted-foreground">
              {{ item.project.description || item.project.key }}
            </p>
            <div class="h-1 overflow-hidden rounded-full bg-muted">
              <div
                class="h-full rounded-full bg-emerald-800/50 dark:bg-emerald-400/60"
                :style="{ width: `${item.total ? (item.completed / item.total) * 100 : 0}%` }"
              />
            </div>
            <div class="mt-2 flex justify-between text-[11px] text-muted-foreground">
              <span>{{ item.completed }} / {{ item.total }} タスク完了</span
              ><span>{{ item.total ? Math.round((item.completed / item.total) * 100) : 0 }}%</span>
            </div>
          </a>
        </div>
        <p v-if="!projects.length" class="py-6 text-sm text-muted-foreground">
          表示できる共有プロジェクトはありません。
        </p>
      </section>
      <DashboardCreateTask
        :key="tenantId"
        v-model:open="createOpen"
        :tenant-id="tenantId"
        :projects="data.projects.map((p) => p.project)"
        @created="refresh"
      />
    </template>
  </div>
</template>

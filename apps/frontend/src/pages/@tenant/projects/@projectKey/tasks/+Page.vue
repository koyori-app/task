<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import type {
  ColumnDef,
  ColumnFiltersState,
  SortingState,
  VisibilityState,
} from '@tanstack/vue-table';
import {
  FlexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  useVueTable,
} from '@tanstack/vue-table';
import type { LucideIcon } from '@lucide/vue';
import { Signal, SignalHigh, SignalLow, SignalMedium } from '@lucide/vue';
import { PhCaretDown, PhCaretUp, PhCaretUpDown } from '@phosphor-icons/vue';
import { computed, h, ref } from 'vue';
import type { Column } from '@tanstack/vue-table';
import { useQuery } from '@tanstack/vue-query';
import { usePageContext } from 'vike-vue/usePageContext';

import { valueUpdater } from '@/components/ui/table/utils';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import AvatarGroup from '@/components/AvatarGroup.vue';
import { fetchClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

// ---- 定数 ----
const LIST_PROJECTS_PATH = '/v1/tenants/{tenant_id}/projects' as const;
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const LIST_STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
const LIST_ASSIGNEES_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/assignees' as const;

// ---- 型定義 ----
type ApiPriority = components['schemas']['TaskPriority'];

interface TaskRow {
  id: string;
  seq_id: number;
  project_key: string;
  title: string;
  status: { id: string; name: string; color: string };
  priority: ApiPriority;
  /** UUID 配列（バックエンド拡充時の差し替えポイント） */
  assignee_user_ids: string[];
  due_date?: string;
}

/** list_assignees の response 型をインライン定義 */
interface AssigneeModel {
  id: string;
  task_id: string;
  user_id: string;
  role: string;
  assigned_at: string;
}

// ---- ページコンテキスト ----
const pageContext = usePageContext();
const tenantId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));

// ---- クエリ①: プロジェクト一覧 ----
const projectsQuery = useQuery({
  queryKey: computed(() => [
    'get',
    LIST_PROJECTS_PATH,
    { params: { path: { tenant_id: tenantId.value } } },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(LIST_PROJECTS_PATH, {
      params: { path: { tenant_id: tenantId.value } },
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value),
});

/** projectKey から project_id を解決 */
const projectId = computed(() => {
  const projects = projectsQuery.data.value;
  if (!projects || !projectKey.value) return null;
  return projects.find((p) => p.key === projectKey.value)?.id ?? null;
});

// ---- クエリ②: タスク一覧 ----
const tasksQuery = useQuery({
  queryKey: computed(() => [
    'get',
    LIST_TASKS_PATH,
    {
      params: {
        path: { tenant_id: tenantId.value, project_id: projectId.value },
        query: { limit: 20, offset: 0 },
      },
    },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(LIST_TASKS_PATH, {
      // query パラメータは openapi-typescript 7.13.0 が正しく operation レベルに生成する
      params: {
        path: { tenant_id: tenantId.value, project_id: projectId.value! },
        query: { limit: 20, offset: 0 },
      },
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value && !!projectId.value),
});

// ---- クエリ③: ステータス一覧 ----
const statusesQuery = useQuery({
  queryKey: computed(() => [
    'get',
    LIST_STATUSES_PATH,
    { params: { path: { tenant_id: tenantId.value, project_id: projectId.value! } } },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(LIST_STATUSES_PATH, {
      params: { path: { tenant_id: tenantId.value, project_id: projectId.value! } },
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value && !!projectId.value),
});

/** status_id → { name, color } 解決用 Map */
const statusMap = computed(() => {
  const statuses = statusesQuery.data.value ?? [];
  return new Map(statuses.map((s) => [s.id, { name: s.name, color: s.color }]));
});

// ---- クエリ④: 担当者一覧（タスク確定後、全タスクを並列取得） ----
const assigneesQuery = useQuery({
  queryKey: computed(() => [
    'assignees',
    tenantId.value,
    projectId.value,
    ...(tasksQuery.data.value?.tasks ?? []).map((t) => t.id),
  ]),
  queryFn: async ({ signal }) => {
    const tasks = tasksQuery.data.value?.tasks ?? [];
    const results = await Promise.all(
      tasks.map((t) =>
        fetchClient
          .GET(LIST_ASSIGNEES_PATH, {
            params: {
              path: { tenant_id: tenantId.value, project_id: projectId.value!, id: t.id },
            },
            signal,
          })
          .then((r) => ({ taskId: t.id, assignees: r.data as AssigneeModel[] | undefined })),
      ),
    );
    const map = new Map<string, string[]>();
    for (const r of results) {
      if (r.assignees) {
        map.set(
          r.taskId,
          r.assignees.map((a) => a.user_id),
        );
        // assignee 情報をログ出力（バックエンド拡充時の参照用）
        console.log({ task_id: r.taskId, assignee_user_ids: r.assignees.map((a) => a.user_id) });
      }
    }
    return map;
  },
  enabled: computed(() => (tasksQuery.data.value?.tasks?.length ?? 0) > 0),
});

// ---- テーブルデータ構築 ----
const taskRows = computed<TaskRow[]>(() => {
  const tasks = tasksQuery.data.value?.tasks;
  const sMap = statusMap.value;
  const aMap = assigneesQuery.data.value;
  if (!tasks) return [];

  return tasks.map((t) => {
    const status = sMap.get(t.status_id) ?? { name: t.status_id, color: '#94a3b8' };
    const assigneeUserIds = aMap?.get(t.id);
    return {
      id: t.id,
      seq_id: t.seq_id,
      project_key: projectKey.value,
      title: t.title,
      status: { id: t.status_id, ...status },
      priority: t.priority,
      assignee_user_ids: assigneeUserIds ?? [],
      due_date: t.soft_deadline ?? undefined,
    };
  });
});

/** 初回ローディング表示。isLoading を使い、初回のみスピナー表示とする。
 *  背景refetch中は古いデータを表示し続ける（isFetching だとrefetch毎にテーブルが
 *  スピナーに置き換わりちらつくため）。refetch中の表示を強化したい場合は別途
 *  インジケーターを追加すること。 */
const isInitialLoading = computed(
  () =>
    projectsQuery.isLoading.value ||
    tasksQuery.isLoading.value ||
    statusesQuery.isLoading.value ||
    assigneesQuery.isLoading.value,
);

const isError = computed(
  () =>
    projectsQuery.isError.value ||
    tasksQuery.isError.value ||
    statusesQuery.isError.value ||
    assigneesQuery.isError.value,
);

// ---- ヘルパー ----
const PRIORITY_ORDER: Record<string, number> = {
  CriticalFire: 0,
  Critical: 1,
  High: 2,
  Medium: 3,
  Low: 4,
  Trivial: 5,
};

/** ソート可能な列ヘッダー: 矢印アイコン付きボタンを返す */
function sortableHeader(column: Column<TaskRow>, label: string) {
  const sorted = column.getIsSorted();
  const icon =
    sorted === 'asc'
      ? h(PhCaretUp, { class: 'ml-1 size-4' })
      : sorted === 'desc'
        ? h(PhCaretDown, { class: 'ml-1 size-4' })
        : h(PhCaretUpDown, { class: 'ml-1 size-4 opacity-40' });
  return h(
    Button,
    {
      variant: 'ghost',
      class: '-ml-3 h-8 text-xs font-medium',
      onClick: () => column.toggleSorting(sorted === 'asc'),
    },
    () => [label, icon],
  );
}

const PRIORITY_CONFIG: Record<ApiPriority, { label: string; color: string; icon: LucideIcon }> = {
  CriticalFire: { label: '緊急', color: '#dc2626', icon: Signal },
  Critical: { label: '重大', color: '#ef4444', icon: Signal },
  High: { label: '高', color: '#f97316', icon: SignalHigh },
  Medium: { label: '中', color: '#eab308', icon: SignalMedium },
  Low: { label: '低', color: '#6b7280', icon: SignalLow },
  Trivial: { label: '些細', color: '#9ca3af', icon: SignalLow },
};

function taskKey(task: TaskRow) {
  return `${task.project_key}-${task.seq_id}`;
}

function formatDate(iso?: string) {
  if (!iso) return null;
  const d = new Date(iso);
  const now = new Date();
  const diff = d.getTime() - now.getTime();
  const days = Math.ceil(diff / 86400000);
  if (days < 0) return { label: `${Math.abs(days)}日超過`, overdue: true };
  if (days === 0) return { label: '今日', overdue: false };
  if (days <= 7) return { label: `${days}日後`, overdue: false };
  return {
    label: d.toLocaleDateString('ja-JP', { month: 'short', day: 'numeric' }),
    overdue: false,
  };
}

// ---- テーブル列定義 ----
const columns: ColumnDef<TaskRow>[] = [
  {
    id: 'select',
    header: ({ table }) =>
      h(Checkbox, {
        modelValue:
          table.getIsAllPageRowsSelected() ||
          (table.getIsSomePageRowsSelected() && 'indeterminate'),
        'onUpdate:modelValue': (value) => table.toggleAllPageRowsSelected(!!value),
        ariaLabel: 'Select all',
      }),
    cell: ({ row }) =>
      h(Checkbox, {
        modelValue: row.getIsSelected(),
        'onUpdate:modelValue': (value) => row.toggleSelected(!!value),
        ariaLabel: 'Select row',
      }),
    enableSorting: false,
    enableHiding: false,
  },
  {
    id: 'key',
    accessorFn: (row) => taskKey(row),
    header: ({ column }) => sortableHeader(column, 'ID'),
    cell: ({ row }) =>
      h(
        'span',
        { class: 'font-mono text-xs text-muted-foreground whitespace-nowrap' },
        taskKey(row.original),
      ),
  },
  {
    accessorKey: 'title',
    header: ({ column }) => sortableHeader(column, 'タイトル'),
    cell: ({ row }) => {
      const task = row.original;
      const pc = PRIORITY_CONFIG[task.priority];
      return h('div', { class: 'flex items-center gap-2 min-w-0' }, [
        h(pc.icon, { class: 'size-4 shrink-0', style: { color: pc.color } }),
        h('span', { class: 'truncate text-sm' }, task.title),
      ]);
    },
  },
  {
    id: 'status',
    accessorFn: (row) => row.status.name,
    header: ({ column }) => sortableHeader(column, 'ステータス'),
    cell: ({ row }) => {
      const s = row.original.status;
      return h(
        'span',
        {
          class:
            'inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium whitespace-nowrap',
          style: {
            backgroundColor: s.color + '1a',
            borderColor: s.color + '66',
            color: s.color,
          },
        },
        s.name,
      );
    },
  },
  {
    id: 'priority',
    accessorFn: (row) => row.priority,
    sortingFn: (a, b) =>
      (PRIORITY_ORDER[a.original.priority] ?? 99) - (PRIORITY_ORDER[b.original.priority] ?? 99),
    header: ({ column }) => sortableHeader(column, '優先度'),
    cell: ({ row }) => {
      const pc = PRIORITY_CONFIG[row.original.priority];
      return h(
        'span',
        {
          class: 'inline-flex items-center gap-1 text-xs whitespace-nowrap',
          style: { color: pc.color },
        },
        [h(pc.icon, { class: 'size-4' }), pc.label],
      );
    },
  },
  {
    id: 'assignee',
    accessorFn: (row) => row.assignee_user_ids[0] ?? '',
    header: ({ column }) => sortableHeader(column, '担当者'),
    cell: ({ row }) => {
      const userIds = row.original.assignee_user_ids;
      if (userIds.length === 0) {
        return h('span', { class: 'text-muted-foreground text-xs' }, '−');
      }
      // TODO: ユーザー名解決 API ができたら user_id → {name, avatar} に差し替え
      return h(AvatarGroup, { userIds, maxDisplay: 3 });
    },
  },
  {
    id: 'due_date',
    accessorFn: (row) => row.due_date ?? '',
    header: ({ column }) => sortableHeader(column, '期限'),
    cell: ({ row }) => {
      const formatted = formatDate(row.original.due_date);
      if (!formatted) return h('span', { class: 'text-muted-foreground text-xs' }, '−');
      return h(
        'span',
        {
          class: [
            'text-xs whitespace-nowrap',
            formatted.overdue ? 'text-red-500 font-medium' : 'text-muted-foreground',
          ],
        },
        formatted.label,
      );
    },
  },
];

// ---- テーブル状態 ----
const sorting = ref<SortingState>([]);
const columnFilters = ref<ColumnFiltersState>([]);
const columnVisibility = ref<VisibilityState>({});
const rowSelection = ref({});

const table = useVueTable({
  get data() {
    return taskRows.value;
  },
  columns,
  getCoreRowModel: getCoreRowModel(),
  getPaginationRowModel: getPaginationRowModel(),
  getSortedRowModel: getSortedRowModel(),
  getFilteredRowModel: getFilteredRowModel(),
  onSortingChange: (u) => valueUpdater(u, sorting),
  onColumnFiltersChange: (u) => valueUpdater(u, columnFilters),
  onColumnVisibilityChange: (u) => valueUpdater(u, columnVisibility),
  onRowSelectionChange: (u) => valueUpdater(u, rowSelection),
  state: {
    get sorting() {
      return sorting.value;
    },
    get columnFilters() {
      return columnFilters.value;
    },
    get columnVisibility() {
      return columnVisibility.value;
    },
    get rowSelection() {
      return rowSelection.value;
    },
  },
});
</script>

<template>
  <div class="flex flex-col gap-3">
    <!-- ローディング / エラー表示 -->
    <div v-if="isInitialLoading" class="flex justify-center py-8">
      <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" />
    </div>

    <div v-else-if="isError" class="flex justify-center py-8 text-sm text-destructive">
      タスクの読み込みに失敗しました
    </div>

    <template v-else>
      <!-- ツールバー -->
      <div class="flex items-center gap-2">
        <Input
          class="h-8 max-w-xs text-sm"
          placeholder="タイトルで絞り込み..."
          :model-value="(table.getColumn('title')?.getFilterValue() as string) ?? ''"
          @update:model-value="table.getColumn('title')?.setFilterValue($event)"
        />
        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="outline" size="sm" class="ml-auto h-8 text-xs">
              列 <PhCaretDown class="ml-1 size-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuCheckboxItem
              v-for="col in table.getAllColumns().filter((c) => c.getCanHide())"
              :key="col.id"
              class="text-sm"
              :model-value="col.getIsVisible()"
              @update:model-value="(v) => col.toggleVisibility(!!v)"
            >
              {{ col.id }}
            </DropdownMenuCheckboxItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <!-- テーブル -->
      <div class="rounded-md border overflow-x-auto">
        <Table>
          <TableHeader>
            <TableRow v-for="hg in table.getHeaderGroups()" :key="hg.id">
              <TableHead v-for="header in hg.headers" :key="header.id" class="h-9 text-xs px-3">
                <FlexRender
                  v-if="!header.isPlaceholder"
                  :render="header.column.columnDef.header"
                  :props="header.getContext()"
                />
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <template v-if="table.getRowModel().rows?.length">
              <TableRow
                v-for="row in table.getRowModel().rows"
                :key="row.id"
                :data-state="row.getIsSelected() && 'selected'"
                class="h-10"
              >
                <TableCell v-for="cell in row.getVisibleCells()" :key="cell.id" class="py-1.5 px-3">
                  <FlexRender :render="cell.column.columnDef.cell" :props="cell.getContext()" />
                </TableCell>
              </TableRow>
            </template>
            <TableRow v-else>
              <TableCell
                :colspan="columns.length"
                class="h-24 text-center text-sm text-muted-foreground"
              >
                タスクが見つかりません
              </TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </div>

      <!-- ページネーション -->
      <div class="flex items-center justify-between text-xs text-muted-foreground">
        <span>
          {{ table.getFilteredSelectedRowModel().rows.length }} /
          {{ table.getFilteredRowModel().rows.length }} 件選択
        </span>
        <div class="flex gap-1.5">
          <Button
            variant="outline"
            size="sm"
            class="h-7 text-xs"
            :disabled="!table.getCanPreviousPage()"
            @click="table.previousPage()"
            >前へ</Button
          >
          <Button
            variant="outline"
            size="sm"
            class="h-7 text-xs"
            :disabled="!table.getCanNextPage()"
            @click="table.nextPage()"
            >次へ</Button
          >
        </div>
      </div>
    </template>
  </div>
</template>

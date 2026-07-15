<script setup lang="ts">
import { Loader2, Signal, SignalHigh, SignalLow, SignalMedium } from '@lucide/vue';
import type { LucideIcon } from '@lucide/vue';
import type {
  ColumnDef,
  ColumnFiltersState,
  PaginationState,
  SortingState,
  VisibilityState,
} from '@tanstack/vue-table';
import {
  FlexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getSortedRowModel,
  useVueTable,
} from '@tanstack/vue-table';
import { PhCaretDown, PhCaretUp, PhCaretUpDown } from '@phosphor-icons/vue';
import { computed, h, ref, watch } from 'vue';
import type { Column } from '@tanstack/vue-table';
import { useQuery, keepPreviousData } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
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
import CreateTaskDialog from '@/components/tasks/CreateTaskDialog.vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { fetchClient } from '@/lib/api-vue-query';
import { formatDeadline, taskDetailHref } from '@/lib/task-display';
import type { components } from '@/generated/api';

// ---- 定数 ----
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const LIST_STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
const TASKS_PAGE_SIZE = 20;

type TasksListQueryKeyParams = {
  params?: {
    path?: { tenant_id?: string; project_id?: string | null };
    query?: { limit?: number; offset?: number };
  };
};

// ---- 型定義 ----
type ApiPriority = components['schemas']['TaskPriority'];
type UserSummary = components['schemas']['UserSummary'];

interface TaskRow {
  id: string;
  seq_id: number;
  project_key: string;
  title: string;
  status: { id: string; name: string; color: string };
  priority: ApiPriority;
  assignees: UserSummary[];
  due_date?: string;
}

// ---- ページコンテキスト ----
const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));
const {
  projectId,
  isProjectNotFound,
  isResolving: isProjectResolving,
  isError: isProjectResolveError,
} = useResolvedProjectId(tenantId, projectKey);

// ---- サーバーサイドページネーション ----
const pagination = ref<PaginationState>({
  pageIndex: 0,
  pageSize: TASKS_PAGE_SIZE,
});

// プロジェクト切替時は先頭ページへ戻す
watch(projectKey, () => {
  pagination.value = { ...pagination.value, pageIndex: 0 };
});

// ---- クエリ②: タスク一覧 ----
const tasksQuery = useQuery({
  queryKey: computed(() => [
    'get',
    LIST_TASKS_PATH,
    {
      params: {
        path: { tenant_id: tenantId.value!, project_id: projectId.value! },
        query: {
          limit: pagination.value.pageSize,
          offset: pagination.value.pageIndex * pagination.value.pageSize,
        },
      },
    },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(LIST_TASKS_PATH, {
      // query パラメータは openapi-typescript 7.13.0 が正しく operation レベルに生成する
      params: {
        path: { tenant_id: tenantId.value!, project_id: projectId.value! },
        query: {
          limit: pagination.value.pageSize,
          offset: pagination.value.pageIndex * pagination.value.pageSize,
        },
      },
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value && !!projectId.value),
  placeholderData: (previousData, previousQuery) => {
    const prevParams = previousQuery?.queryKey[2] as TasksListQueryKeyParams | undefined;
    const prevProjectId = prevParams?.params?.path?.project_id;
    if (prevProjectId && projectId.value && prevProjectId === projectId.value) {
      return keepPreviousData(previousData);
    }
    return undefined;
  },
});

const taskTotal = computed(() => tasksQuery.data.value?.total ?? 0);
const isCreateDialogOpen = ref(false);

// ---- クエリ③: ステータス一覧 ----
const statusesQuery = useQuery({
  queryKey: computed(() => [
    'get',
    LIST_STATUSES_PATH,
    { params: { path: { tenant_id: tenantId.value!, project_id: projectId.value! } } },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(LIST_STATUSES_PATH, {
      params: { path: { tenant_id: tenantId.value!, project_id: projectId.value! } },
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

// ---- テーブルデータ構築 ----
const taskRows = computed<TaskRow[]>(() => {
  const tasks = tasksQuery.data.value?.tasks;
  const sMap = statusMap.value;
  if (!tasks) return [];

  return tasks.map((t) => {
    const status = sMap.get(t.status_id) ?? { name: t.status_id, color: '#94a3b8' };
    return {
      id: t.id,
      seq_id: t.seq_id,
      project_key: projectKey.value,
      title: t.title,
      status: { id: t.status_id, ...status },
      priority: t.priority,
      assignees: t.assignees.map((a) => a.user),
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
    isTenantResolving.value ||
    isProjectResolving.value ||
    tasksQuery.isLoading.value ||
    statusesQuery.isLoading.value,
);

const isError = computed(
  () =>
    isTenantResolveError.value ||
    isProjectResolveError.value ||
    tasksQuery.isError.value ||
    statusesQuery.isError.value,
);

// ---- ヘルパー ----
const PRIORITY_ORDER: Record<ApiPriority, number> = {
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

function navigateToTask(task: TaskRow, event: MouseEvent) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  event.preventDefault();
  void navigate(taskDetailHref(tenantDisplayId.value, projectKey.value, task.seq_id));
}

// ---- テーブル列定義 ----
const columns: ColumnDef<TaskRow>[] = [
  {
    id: 'select',
    header: ({ table }) =>
      h(Checkbox, {
        class: 'relative z-10',
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
      const href = taskDetailHref(tenantDisplayId.value, projectKey.value, task.seq_id);
      return h('div', { class: 'flex items-center gap-2 min-w-0' }, [
        h(pc.icon, { class: 'size-4 shrink-0', style: { color: pc.color } }),
        h(
          'a',
          {
            href,
            class:
              "truncate text-sm text-primary hover:underline after:absolute after:inset-0 after:content-['']",
            onClick: (event: MouseEvent) => navigateToTask(task, event),
          },
          task.title,
        ),
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
    sortingFn: (a, b) => PRIORITY_ORDER[a.original.priority] - PRIORITY_ORDER[b.original.priority],
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
    accessorFn: (row) => row.assignees[0]?.username ?? '',
    header: ({ column }) => sortableHeader(column, '担当者'),
    cell: ({ row }) => {
      const users = row.original.assignees;
      if (users.length === 0) {
        return h('span', { class: 'text-muted-foreground text-xs' }, '−');
      }
      return h(AvatarGroup, { users, maxDisplay: 3 });
    },
  },
  {
    id: 'due_date',
    accessorFn: (row) => row.due_date ?? '',
    header: ({ column }) => sortableHeader(column, '期限'),
    cell: ({ row }) => {
      const formatted = formatDeadline(row.original.due_date);
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
  getRowId: (row) => row.id,
  getCoreRowModel: getCoreRowModel(),
  getSortedRowModel: getSortedRowModel(),
  getFilteredRowModel: getFilteredRowModel(),
  manualPagination: true,
  get rowCount() {
    return taskTotal.value;
  },
  onSortingChange: (u) => valueUpdater(u, sorting),
  onColumnFiltersChange: (u) => valueUpdater(u, columnFilters),
  onColumnVisibilityChange: (u) => valueUpdater(u, columnVisibility),
  onRowSelectionChange: (u) => valueUpdater(u, rowSelection),
  onPaginationChange: (u) => valueUpdater(u, pagination),
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
    get pagination() {
      return pagination.value;
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

    <div
      v-else-if="isTenantNotFound"
      class="flex justify-center py-8 text-sm text-muted-foreground"
    >
      テナントが見つかりません
    </div>

    <div
      v-else-if="isProjectNotFound"
      class="flex justify-center py-8 text-sm text-muted-foreground"
    >
      プロジェクトが見つかりません
    </div>

    <template v-else>
      <!-- ツールバー（ソート・タイトル絞り込みは現在ページ内の行のみ対象。サーバー側未対応） -->
      <div class="flex items-center gap-2">
        <Input
          class="h-8 max-w-xs text-sm"
          placeholder="タイトルで絞り込み..."
          :model-value="(table.getColumn('title')?.getFilterValue() as string) ?? ''"
          @update:model-value="table.getColumn('title')?.setFilterValue($event)"
        />
        <Button size="sm" class="ml-auto h-8 text-xs" @click="isCreateDialogOpen = true">
          新規タスク
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="outline" size="sm" class="h-8 text-xs">
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

      <CreateTaskDialog
        v-if="tenantId && projectId"
        v-model:open="isCreateDialogOpen"
        :tenant-id="tenantId"
        :tenant-display-id="tenantDisplayId"
        :project-id="projectId"
        :project-key="projectKey"
        :statuses="statusesQuery.data.value ?? []"
      />

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
                class="relative h-10"
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

      <!-- ページネーション（API total 連動のサーバーサイド） -->
      <div class="flex items-center justify-between text-xs text-muted-foreground">
        <span>
          {{ table.getFilteredSelectedRowModel().rows.length }} / {{ taskTotal }} 件選択
        </span>
        <div class="flex items-center gap-2">
          <span>
            {{ taskTotal === 0 ? 0 : pagination.pageIndex * pagination.pageSize + 1 }}–{{
              Math.min((pagination.pageIndex + 1) * pagination.pageSize, taskTotal)
            }}
            / {{ taskTotal }} 件
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
      </div>
    </template>
  </div>
</template>

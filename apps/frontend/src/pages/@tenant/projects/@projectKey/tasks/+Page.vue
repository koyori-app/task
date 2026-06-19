<script setup lang="ts">
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
import { h, ref } from 'vue';
import type { Column } from '@tanstack/vue-table';

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
import { Avatar, AvatarFallback } from '@/components/ui/avatar';

// ---- 型定義 ----
type Priority = 'urgent' | 'high' | 'medium' | 'low';

interface TaskStatus {
  id: string;
  name: string;
  color: string;
}

interface TaskAssignee {
  name: string;
  initials: string;
}

interface Task {
  id: string;
  seq_id: number;
  project_key: string;
  title: string;
  status: TaskStatus;
  priority: Priority;
  assignee?: TaskAssignee;
  due_date?: string;
}

// ---- ヘルパー ----
const PRIORITY_ORDER: Record<Priority, number> = { urgent: 0, high: 1, medium: 2, low: 3 };

/** ソート可能な列ヘッダー: 矢印アイコン付きボタンを返す */
function sortableHeader(column: Column<Task>, label: string) {
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

const PRIORITY_CONFIG: Record<Priority, { label: string; color: string; icon: LucideIcon }> = {
  urgent: { label: '緊急', color: '#ef4444', icon: Signal },
  high: { label: '高', color: '#f97316', icon: SignalHigh },
  medium: { label: '中', color: '#eab308', icon: SignalMedium },
  low: { label: '低', color: '#6b7280', icon: SignalLow },
};

function taskKey(task: Task) {
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

// ---- モックデータ ----
const data: Task[] = [
  {
    id: '1',
    seq_id: 1,
    project_key: 'ENG',
    title: 'OAuth 対応を実装する',
    status: { id: 's1', name: 'In Progress', color: '#3b82f6' },
    priority: 'high',
    assignee: { name: '田中 太郎', initials: '田' },
    due_date: new Date(Date.now() + 2 * 86400000).toISOString(),
  },
  {
    id: '2',
    seq_id: 2,
    project_key: 'ENG',
    title: 'ログイン画面の UI 実装',
    status: { id: 's2', name: 'In Review', color: '#8b5cf6' },
    priority: 'medium',
    assignee: { name: '鈴木 花子', initials: '鈴' },
    due_date: new Date(Date.now() - 1 * 86400000).toISOString(),
  },
  {
    id: '3',
    seq_id: 3,
    project_key: 'ENG',
    title: 'DB スキーマ設計',
    status: { id: 's3', name: 'Done', color: '#22c55e' },
    priority: 'urgent',
    assignee: { name: '山田 次郎', initials: '山' },
  },
  {
    id: '4',
    seq_id: 4,
    project_key: 'ENG',
    title: '通知メール送信機能',
    status: { id: 's1', name: 'In Progress', color: '#3b82f6' },
    priority: 'low',
    due_date: new Date(Date.now() + 14 * 86400000).toISOString(),
  },
  {
    id: '5',
    seq_id: 5,
    project_key: 'ENG',
    title: 'タスク一覧 API の実装',
    status: { id: 's0', name: 'Backlog', color: '#94a3b8' },
    priority: 'medium',
    assignee: { name: '田中 太郎', initials: '田' },
  },
];

// ---- テーブル列定義 ----
const columns: ColumnDef<Task>[] = [
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
    accessorFn: (row) => row.assignee?.name ?? '',
    header: ({ column }) => sortableHeader(column, '担当者'),
    cell: ({ row }) => {
      const a = row.original.assignee;
      if (!a) return h('span', { class: 'text-muted-foreground text-xs' }, '−');
      return h('div', { class: 'flex items-center gap-1.5' }, [
        h(Avatar, { class: 'size-5' }, () => [
          h(AvatarFallback, { class: 'text-[10px]' }, () => a.initials),
        ]),
        h('span', { class: 'text-xs truncate max-w-24' }, a.name),
      ]);
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
  data,
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
  </div>
</template>

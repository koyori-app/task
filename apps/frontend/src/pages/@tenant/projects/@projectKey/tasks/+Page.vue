<script setup lang="ts">
import { Loader2, Search, Signal, SignalHigh, SignalLow, SignalMedium, X } from '@lucide/vue';
import type { LucideIcon } from '@lucide/vue';
import type {
  ColumnDef,
  ColumnFiltersState,
  ColumnVisibilityState,
  PaginationState,
  SortingState,
} from '@tanstack/vue-table';
import {
  FlexRender,
  columnFilteringFeature,
  columnVisibilityFeature,
  createFilteredRowModel,
  createSortedRowModel,
  filterFns,
  functionalUpdate,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
  sortFns,
  tableFeatures,
  useTable,
} from '@tanstack/vue-table';
import { PhRows, PhTable } from '@phosphor-icons/vue';
import { computed, h, onUnmounted, ref, watch, type Component } from 'vue';
import { useQuery, keepPreviousData } from '@tanstack/vue-query';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';
import { useMediaQuery } from '@vueuse/core';

import {
  AvatarGroup,
  Button,
  Checkbox,
  DataList,
  DataListRow,
  Dropdown,
  Input,
  Pagination,
  Spinner,
  type DataListColumn,
  type DataListProps,
  type DropdownItem,
} from '@koyori-app/ui-vue';
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable';
import TaskDetailPane, { type TaskRefLink } from '@/components/tasks/TaskDetailPane.vue';
import CreateTaskDialog from '@/components/tasks/CreateTaskDialog.vue';
import TaskTitleLink from '@/components/tasks/TaskTitleLink.vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { fetchClient, taskSearchQueryOptions, useAssignableUsersQuery } from '@/lib/api-vue-query';
import { formatDeadline, taskDetailHref, taskSeqKey } from '@/lib/task-display';
import type { components } from '@/generated/api';
import {
  buildTasksListQueryParams,
  taskListPlaceholderData,
  useTaskLabelFilter,
  watchAvailableTaskLabels,
} from './task-list-label-filter';
import { popOverlayParent, pushOverlayParent } from '@/components/tasks/overlay-parent-stack';
import {
  parseTaskListUrlState,
  useTaskListUrlSync,
  type TaskListView,
} from './task-list-url-state';
import TaskGroupedList from '@/components/tasks/TaskGroupedList.vue';
import TaskGroupQuery from '@/components/tasks/TaskGroupQuery.vue';
import type { TaskGroup } from '@/components/tasks/task-grouped-columns';
import TaskDetailOverlay from '@/components/tasks/TaskDetailOverlay.vue';
import { useTaskRowMutations } from '@/composables/useTaskRowMutations';
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts';
import { taskListApiSort, type TaskListSortingState } from '@/components/tasks/task-list-sort';

// ---- 定数 ----
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const LIST_STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
/** List 表示でステータスごとに最初に取る件数。「もっと見る」でこの単位ずつ増やす */
const GROUP_PAGE_SIZE = 20;
const LIST_LABELS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/labels' as const;
const TASKS_PAGE_SIZE = 20;
const SEARCH_PAGE_SIZE = 50;
const SEARCH_DEBOUNCE_MS = 300;

type TaskSearchQueryKeyParams = {
  params?: {
    path?: { tenant_id?: string; project_id?: string };
  };
};

// ---- 型定義 ----
type ApiPriority = components['schemas']['TaskPriority'];
type UserSummary = components['schemas']['UserSummary'];
type TaskLabel = components['schemas']['LabelResponse'];
type TaskResponse = components['schemas']['TaskResponse'];

interface TaskRow {
  id: string;
  seq_id: number;
  project_key: string;
  title: string;
  status: { id: string; name: string; color: string };
  priority: ApiPriority;
  assignees: UserSummary[];
  labels: TaskLabel[];
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

// ---- 分割ビュー: 選択タスクと詳細ペイン ----
// selectedTaskId は URL/詳細ページと同形の seq key（例: "ENG-42"）を保持する。
// これにより詳細クエリのキャッシュがフルページ詳細（@taskId）と共有される。
const selectedTaskId = ref<string | null>(null);
// オーバーレイの中で子タスクへ移ったときの戻り先（overlay-parent-stack）。
// 一覧の行・タスク作成・プロジェクト切替のように外から選択が変わったら捨てる。
const overlayParentStack = ref<string[]>([]);

// 広い画面でのみ inline 分割を出す。狭い画面は従来どおり詳細ページへ遷移させる。
const canInline = useMediaQuery('(min-width: 1024px)');
// Table 表示だけが右ペインを出す。List 表示の詳細はオーバーレイ（overlayTaskSeqKey）。
const showDetail = computed(() => canInline.value && !!selectedTaskId.value && !isListView.value);

// 一覧の状態（選択・ページ・検索語・ラベル・並び替え）を URL クエリから読む
// （クライアントのみ）。復元先の ref はこの後で定義するので、値だけ先に取る。
const initialUrlSearch = import.meta.env.SSR
  ? undefined
  : (pageContext as { urlParsed?: { search?: Record<string, string> } } | undefined)?.urlParsed
      ?.search;
const initialListState = parseTaskListUrlState(initialUrlSearch);
if (!import.meta.env.SSR) {
  const initialSelected = initialUrlSearch?.selected;
  if (initialSelected) selectedTaskId.value = initialSelected;
}

// プロジェクト切替時は選択を解除する（別プロジェクトのタスクを指したままにしない）。
watch(projectKey, () => {
  overlayParentStack.value = [];
  selectedTaskId.value = null;
});

/**
 * 行のクリック。広い画面では右ペインで開き、狭い画面は詳細ページへ送る。
 *
 * **判定は描画時ではなくクリック時に読む。** `useMediaQuery` は
 * `useSupported`（内部で `useMounted`）に依存するためマウント前は必ず false で、
 * マウント後に true へ変わる。この値を列定義の `cell` へ焼き込むと、TanStack の
 * `FlexRender` がセルを描き直さず false のまま固まり、広い画面なのに詳細ページへ
 * 飛ぶ（右ペインは出ているのに一覧のクリックだけ遷移する形で本番で発生した）。
 */
function onSelectRow(seqId: number) {
  if (!canInline.value) {
    void navigate(taskDetailHref(tenantDisplayId.value, projectKey.value, seqId));
    return;
  }
  overlayParentStack.value = [];
  selectedTaskId.value = taskSeqKey(projectKey.value, seqId);
}

function closeDetail() {
  selectedTaskId.value = null;
}

function isRowActive(seqId: number) {
  return !!selectedTaskId.value && taskSeqKey(projectKey.value, seqId) === selectedTaskId.value;
}

// ---- サーバー側検索 ----
// ---- 表示形式（既定は List）----
const VIEW_OPTIONS: { value: TaskListView; label: string; icon: Component }[] = [
  { value: 'list', label: 'List', icon: PhRows },
  { value: 'table', label: 'Table', icon: PhTable },
];
const view = ref<TaskListView>(initialListState.view);
const isListView = computed(() => view.value === 'list');

const searchInput = ref(initialListState.q);
const submittedSearchQuery = ref(initialListState.q);
let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

function updateSubmittedSearchQuery() {
  submittedSearchQuery.value = searchInput.value.trim();
}

function scheduleSearch(value: string | number) {
  if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
  searchInput.value = String(value);
  if (!searchInput.value.trim()) {
    submittedSearchQuery.value = '';
    return;
  }
  searchDebounceTimer = setTimeout(updateSubmittedSearchQuery, SEARCH_DEBOUNCE_MS);
}

function submitSearch() {
  if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
  updateSubmittedSearchQuery();
}

function clearSearch() {
  if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
  searchInput.value = '';
  submittedSearchQuery.value = '';
}

onUnmounted(() => {
  if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
});

const taskSearchQuery = useQuery(
  computed(() => ({
    ...taskSearchQueryOptions(
      tenantId.value ?? '',
      projectId.value ?? '',
      submittedSearchQuery.value,
      { limit: SEARCH_PAGE_SIZE, offset: 0 },
    ),
    enabled: !!tenantId.value && !!projectId.value && !!submittedSearchQuery.value,
    placeholderData: (previousData, previousQuery) => {
      const previousParams = previousQuery?.queryKey[2] as TaskSearchQueryKeyParams | undefined;
      const previousPath = previousParams?.params?.path;
      if (
        previousPath?.tenant_id === tenantId.value &&
        previousPath.project_id === projectId.value
      ) {
        return keepPreviousData(previousData);
      }
      return undefined;
    },
  })),
);

const isSearchActive = computed(() => !!submittedSearchQuery.value);

// ---- サーバーサイドページネーション ----
const pagination = ref<PaginationState>({
  // URL は 1 始まり（人が読む値）、TanStack は 0 始まり
  pageIndex: initialListState.page - 1,
  pageSize: TASKS_PAGE_SIZE,
});

// プロジェクト切替時は先頭ページへ戻す
watch(projectKey, () => {
  pagination.value = { ...pagination.value, pageIndex: 0 };
});

// ---- ラベルフィルタ ----
// null は「すべて」。切り替え時は先頭ページへ戻す
const { selectedLabelId } = useTaskLabelFilter(pagination, projectKey, initialListState.labelId);

// ---- クエリ②: タスク一覧 ----
/**
 * Table 表示の一覧（ページャつき）を使うか。
 *
 * List 表示はステータス別の `groupQueries` から作るので、この一覧は使わない。
 * 使わないまま取りに行くと、余分なリクエストが増えるだけでなく、ローディングと
 * エラーがこの query に紐付いているために List が出せない・List 全体が
 * エラー画面になる。検索中も表示に使わないので同じく止める。
 */
const usesTaskList = computed(() => !isListView.value && !isSearchActive.value);

const tasksQuery = useQuery({
  queryKey: computed(() => [
    'get',
    LIST_TASKS_PATH,
    buildTasksListQueryParams(
      tenantId.value!,
      projectId.value!,
      pagination.value,
      selectedLabelId.value,
    ),
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(LIST_TASKS_PATH, {
      // query パラメータは openapi-typescript 7.13.0 が正しく operation レベルに生成する
      ...buildTasksListQueryParams(
        tenantId.value!,
        projectId.value!,
        pagination.value,
        selectedLabelId.value,
      ),
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value && !!projectId.value && usesTaskList.value),
  placeholderData: (previousData, previousQuery) => {
    // ラベルフィルタが変わったときは旧条件のデータを見せない（ページング時のみ維持）
    return taskListPlaceholderData(
      previousData,
      previousQuery,
      projectId.value,
      selectedLabelId.value,
    );
  },
});

// 取得済みの総件数。未取得は null にして「0 件だった」と区別する
// （同じ 0 だと範囲外ページの丸めが走らない経路ができる。一覧の件数とページャも未取得なら出さない）
const fetchedTaskTotal = computed(() => tasksQuery.data.value?.total ?? null);
/** 表示用。未取得は 0 件として出す */
const taskTotal = computed(() => fetchedTaskTotal.value ?? 0);
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

// ---- クエリ④: ラベル一覧（フィルタ用） ----
const labelsQuery = useQuery({
  queryKey: computed(() => [
    'get',
    LIST_LABELS_PATH,
    { params: { path: { tenant_id: tenantId.value!, project_id: projectId.value! } } },
  ]),
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(LIST_LABELS_PATH, {
      params: { path: { tenant_id: tenantId.value!, project_id: projectId.value! } },
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => !!tenantId.value && !!projectId.value),
});

// ラベルも未取得を null で区別する（表示用は空配列へ畳む）
const fetchedProjectLabels = computed(() => labelsQuery.data.value ?? null);
const projectLabels = computed(() => fetchedProjectLabels.value ?? []);

// ---- クエリ④: 担当者候補（List 表示の行から割り当てる）----
// 担当者の候補が要るのは List 表示のピッカーと作成モーダル。どちらも出ていなければ
// 取りに行かない（Table 表示でモーダルを閉じたままなら問い合わせは発生しない）
const needsAssignableUsers = computed(() => isListView.value || isCreateDialogOpen.value);
const membersQuery = useAssignableUsersQuery(
  () => (needsAssignableUsers.value ? tenantId.value : null),
  () => (needsAssignableUsers.value ? projectId.value : null),
);

const projectMembers = computed(() => membersQuery.data.value ?? []);
// 候補の取得状態はピッカーへ渡す（取得中・失敗を「候補 0 人」と混ぜない）
const projectMembersState = computed(() => ({
  loading: membersQuery.isLoading.value,
  error: membersQuery.isError.value,
  onRetry: () => void membersQuery.refetch(),
}));

// ---- List 表示: ステータスごとの取得 ----
// 1 本の一覧クエリを画面側で仕分けると、グループの件数がページ内の件数になってしまう
// （「Todo 3」なのに実際は 40 件、など）。ステータスで絞った問い合わせを並べ、
// 件数はサーバの total を使う。
//
// 「もっと見る」は limit を伸ばすのではなくページを足す。limit を伸ばす方式だと
// サーバ側の上限（tasks.rs の `clamp(1, 200)`）に当たった時点で 201 件目以降へ
// 進めなくなり、押すたびに同じ 200 件を取り直すだけになる。ページ単位なら取得済みの
// ページがキャッシュに残るので、押した瞬間に行が消える問題も起きない。
//
// ページの継ぎ目は offset ではなくサーバの next_cursor で持つ。offset だと、
// 1 ページ目を読んだ後にタスクが 1 件でも他のステータスへ移ると後続の境界が詰まり、
// 境界のタスクがどのページにも現れない（重複は ID で落とせるが、欠落は戻らない）。
//
// カーソルの列はこの画面では持たず、ステータスごとの TaskGroupQuery（infinite query）に
// 持たせる。ページを 1 本ずつ別クエリにすると後続ページのキーに取得時点のカーソルが
// 焼き付き、window focus や外からの invalidate で先頭ページだけ中身が変わったときに、
// 後続が古い鍵のまま引いて境界のタスクを落とす。infinite query はページを順に引き直して
// 鍵を採り直すので、取り直しの経路を数え上げなくてよい。
const workflowStatuses = computed(() => statusesQuery.data.value ?? []);

/** TaskGroupQuery から受け取った、ステータス id ごとの塊。 */
const groupByStatusId = ref<Record<string, TaskGroup>>({});

function setTaskGroup(group: TaskGroup) {
  groupByStatusId.value = { ...groupByStatusId.value, [group.status.id]: group };
}

const taskGroups = computed<TaskGroup[]>(() =>
  workflowStatuses.value.flatMap((status) => groupByStatusId.value[status.id] ?? []),
);

function loadMoreInGroup(statusId: string) {
  groupByStatusId.value[statusId]?.loadMore();
}

// ---- List 表示: 行からの更新 ----
const rowMutations = useTaskRowMutations({
  tenantId: () => tenantId.value,
  projectId: () => projectId.value,
});
// ---- List 表示: 詳細のオーバーレイ ----
//
// 選択は表示形式によらず selectedTaskId に一本化し、List 表示のときだけ
// オーバーレイとして読み替える。ここを別の ref に分けると、検索結果のクリックや
// タスク作成のように selectedTaskId しか触らない経路が List 表示で無反応になる。
const overlayTaskSeqKey = computed(() =>
  isListView.value && canInline.value ? selectedTaskId.value : null,
);

/**
 * 閉じるアニメーションの間だけ、直前に開いていたタスクを保持する。
 *
 * 選択を null にした瞬間に v-if でオーバーレイごと消すと、Dialog の退場
 * アニメーションが再生されないまま画面から消える。
 */
const overlayRenderedTaskSeqKey = ref<string | null>(null);
let overlayCleanupTimer: ReturnType<typeof setTimeout> | undefined;

watch(
  overlayTaskSeqKey,
  (seqKey) => {
    if (overlayCleanupTimer) clearTimeout(overlayCleanupTimer);
    if (seqKey) {
      overlayRenderedTaskSeqKey.value = seqKey;
      return;
    }
    // Dialog の duration-200 に合わせて、閉じ切ってから外す
    overlayCleanupTimer = setTimeout(() => {
      overlayRenderedTaskSeqKey.value = null;
    }, 250);
  },
  // URL の ?selected= から復元した選択も拾う（初回は watch が走らないため）
  { immediate: true },
);

onUnmounted(() => {
  if (overlayCleanupTimer) clearTimeout(overlayCleanupTimer);
});

function openOverlay(task: TaskResponse) {
  onSelectRow(task.seq_id);
}

/** オーバーレイの中から別タスクを開く。戻れるように今のタスクを積む */
function openTaskFromOverlay(task: TaskResponse) {
  openInOverlay(taskSeqKey(projectKey.value, task.seq_id));
}

/** オーバーレイの中の説明の KEY-N。子タスクと同じく積んで開く */
function openTaskRefFromOverlay(ref: TaskRefLink) {
  // 一覧と ?selected= は 1 プロジェクト分なので、別プロジェクトのタスクは詳細ページへ送る
  if (ref.projectKey !== projectKey.value) return void navigate(ref.href);
  openInOverlay(ref.seqKey);
}

/** 分割ビューの説明の KEY-N。子タスクと同じく右ペインの選択を差し替える */
function openTaskRefInPane(ref: TaskRefLink) {
  if (ref.projectKey !== projectKey.value) return void navigate(ref.href);
  selectedTaskId.value = ref.seqKey;
}

function openInOverlay(next: string) {
  overlayParentStack.value = pushOverlayParent(
    overlayParentStack.value,
    selectedTaskId.value,
    next,
  );
  // オーバーレイが出ている = canInline なので、onSelectRow の詳細ページ送りは要らない
  selectedTaskId.value = next;
}

/** Esc・外側クリック・× で閉じる。子から開いていたら親へ 1 段戻る */
function onOverlayOpenChange(open: boolean) {
  if (open) return;
  const { stack, next } = popOverlayParent(overlayParentStack.value);
  overlayParentStack.value = stack;
  selectedTaskId.value = next;
}

watchAvailableTaskLabels(selectedLabelId, fetchedProjectLabels);
const selectedLabelName = computed(
  () => projectLabels.value.find((label) => label.id === selectedLabelId.value)?.name ?? null,
);

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
      labels: t.labels,
      due_date: t.soft_deadline ?? undefined,
    };
  });
});

/** 初回ローディング表示。Table 用一覧の取得中・失敗はここで画面を止めず、
 *  一覧（DataList）の status で出す（taskListStatus）。 */
const isInitialLoading = computed(
  () => isTenantResolving.value || isProjectResolving.value || statusesQuery.isLoading.value,
);

const isError = computed(
  () => isTenantResolveError.value || isProjectResolveError.value || statusesQuery.isError.value,
);

/** 一覧本体（作成ボタンと作成ダイアログを含む分岐）が出ているか。T のショートカットとも共有する */
const canCreate = computed(
  () =>
    !isInitialLoading.value &&
    !isError.value &&
    !isTenantNotFound.value &&
    !isProjectNotFound.value,
);
useKeyboardShortcuts({
  t: () => {
    if (canCreate.value) isCreateDialogOpen.value = true;
  },
});

// ---- ヘルパー ----
const PRIORITY_ORDER: Record<ApiPriority, number> = {
  CriticalFire: 0,
  Critical: 1,
  High: 2,
  Medium: 3,
  Low: 4,
  Trivial: 5,
};

// ---- テーブル機能（v9） ----
// v9 は使う機能を features として先に束ね、その型が Column / ColumnDef /
// Table の第一型引数になる（機能ごとに state・options・API が型に乗る造り）。
// row model の工場（sorted/filtered）と、列が名前で引く sort/filter 関数の
// 登録もこの束が運ぶ。coreRowModel は組み込みの既定があるため書かない。
const features = tableFeatures({
  columnFilteringFeature,
  columnVisibilityFeature,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
  filteredRowModel: createFilteredRowModel(),
  sortedRowModel: createSortedRowModel(),
  filterFns,
  sortFns,
});
type TaskTableFeatures = typeof features;

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

type CreatedTask = components['schemas']['TaskDetailResponse'];

function onTaskCreated(task: CreatedTask) {
  isCreateDialogOpen.value = false;
  // 分割ビューが出せる画面では作成タスクを右ペインで開く。狭い画面は詳細ページへ遷移。
  if (canInline.value) {
    overlayParentStack.value = [];
    selectedTaskId.value = taskSeqKey(projectKey.value, task.seq_id);
    return;
  }
  void navigate(taskDetailHref(tenantDisplayId.value, projectKey.value, task.seq_id));
}

// ---- テーブル列定義 ----
const columns: ColumnDef<TaskTableFeatures, TaskRow>[] = [
  {
    id: 'select',
    // 見出しはラベルだけ。DataList の見出しに部品を置けないので、すべて選択は一覧の下に置く
    header: '選択',
    cell: ({ row }) =>
      h(Checkbox, {
        label: `${taskKey(row.original)} を選択`,
        hideLabel: true,
        checked: row.getIsSelected(),
        onCheckedChange: (checked: boolean) => row.toggleSelected(checked),
      }),
    enableSorting: false,
    enableHiding: false,
  },
  {
    id: 'key',
    accessorFn: (row) => taskKey(row),
    header: 'ID',
    cell: ({ row }) =>
      h(
        'span',
        { class: 'font-mono text-xs text-muted-foreground whitespace-nowrap' },
        taskKey(row.original),
      ),
  },
  {
    accessorKey: 'title',
    header: 'タイトル',
    cell: ({ row }) => {
      const task = row.original;
      const pc = PRIORITY_CONFIG[task.priority];
      return h('div', { class: 'flex items-center gap-2 min-w-0' }, [
        h(pc.icon, { class: 'size-4 shrink-0', style: { color: pc.color } }),
        h(TaskTitleLink, {
          tenantDisplayId: tenantDisplayId.value,
          projectKey: projectKey.value,
          seqId: task.seq_id,
          title: task.title,
          onSelect: onSelectRow,
        }),
      ]);
    },
  },
  {
    id: 'status',
    accessorFn: (row) => row.status.name,
    header: 'ステータス',
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
    sortFn: (a, b) => PRIORITY_ORDER[a.original.priority] - PRIORITY_ORDER[b.original.priority],
    header: '優先度',
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
    id: 'labels',
    enableSorting: false,
    header: 'ラベル',
    cell: ({ row }) => {
      const labels = row.original.labels;
      if (labels.length === 0) {
        return h('span', { class: 'text-muted-foreground text-xs' }, '−');
      }
      return h(
        'div',
        { class: 'flex flex-wrap gap-1' },
        labels.map((label) =>
          h(
            'span',
            {
              key: label.id,
              class:
                'inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium whitespace-nowrap',
              style: {
                backgroundColor: label.color + '1a',
                borderColor: label.color + '66',
                color: label.color,
              },
            },
            label.name,
          ),
        ),
      );
    },
  },
  {
    id: 'assignee',
    accessorFn: (row) => row.assignees[0]?.username ?? '',
    header: '担当者',
    cell: ({ row }) => {
      const users = row.original.assignees;
      if (users.length === 0) {
        return h('span', { class: 'text-muted-foreground text-xs' }, '−');
      }
      const max = 3;
      const first = users[0]?.username ?? '';
      return h('div', { class: 'flex items-center gap-1.5' }, [
        h(AvatarGroup, {
          label: '担当者',
          items: users.map((user) => ({ name: user.username, src: user.avatar_url ?? undefined })),
          max,
          size: 28,
        }),
        // 先頭名 + 他N名（殿指示により維持。N は +N と同じ数）
        h(
          'span',
          { class: 'max-w-28 truncate text-xs text-muted-foreground' },
          users.length > max ? `${first} 他${users.length - max}名` : first,
        ),
      ]);
    },
  },
  {
    id: 'due_date',
    accessorFn: (row) => row.due_date ?? '',
    header: '期限',
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
const sorting = ref<SortingState>(initialListState.sorting);
const taskGroupApiSort = computed(() => taskListApiSort(sorting.value));

function onListSortingChange(next: TaskListSortingState) {
  sorting.value = next;
}
const columnFilters = ref<ColumnFiltersState>([]);
const columnVisibility = ref<ColumnVisibilityState>({});
const rowSelection = ref({});

// ---- URL 同期 ----
// 選択・ページ・検索語・ラベル・並び替えを URL へ書き戻し、件数が分かったら
// 範囲外のページを丸める（配線ごと task-list-url-state に寄せてテストしている）。
useTaskListUrlSync({
  selectedTaskId,
  view,
  pagination,
  submittedSearchQuery,
  selectedLabelId,
  sorting,
  taskTotal: fetchedTaskTotal,
  isPagerActive: usesTaskList,
});

const table = useTable({
  features,
  get data() {
    return taskRows.value;
  },
  columns,
  getRowId: (row: TaskRow) => row.id,
  manualPagination: true,
  get rowCount() {
    return taskTotal.value;
  },
  onSortingChange: (u) => (sorting.value = functionalUpdate(u, sorting.value)),
  onColumnFiltersChange: (u) => (columnFilters.value = functionalUpdate(u, columnFilters.value)),
  onColumnVisibilityChange: (u) =>
    (columnVisibility.value = functionalUpdate(u, columnVisibility.value)),
  onRowSelectionChange: (u) => (rowSelection.value = functionalUpdate(u, rowSelection.value)),
  onPaginationChange: (u) => (pagination.value = functionalUpdate(u, pagination.value)),
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

// ---- Table 表示の描画（DataList）----
// 状態の持ち主は TanStack Table のまま。DataList は並べ替えもページングもしない。

/** table-layout: fixed なので、幅を決めない列（タイトル）が残りを使う */
const TASK_COLUMN_WIDTHS: Record<string, string> = {
  select: '3.5rem',
  key: '6rem',
  status: '8rem',
  priority: '6rem',
  labels: '10rem',
  assignee: '12rem',
  due_date: '7rem',
};

const taskListColumns = computed<DataListColumn[]>(() =>
  table.getVisibleLeafColumns().map((column) => ({
    id: column.id,
    label: String(column.columnDef.header ?? column.id),
    sortable: column.getCanSort(),
    width: TASK_COLUMN_WIDTHS[column.id],
  })),
);

const taskListSort = computed<DataListProps['sort']>(() => {
  const current = sorting.value[0];
  return current
    ? { columnId: current.id, direction: current.desc ? 'descending' : 'ascending' }
    : null;
});

function onTaskListSortChange(next: DataListProps['sort']) {
  sorting.value = next ? [{ id: next.columnId, desc: next.direction === 'descending' }] : [];
}

/** 初回・ページ送り・再試行の取得中は loading。データがある裏の再取得では一覧を出し続ける */
const taskListStatus = computed<DataListProps['status']>(() => {
  const { isFetching, isError, isPlaceholderData, data } = tasksQuery;
  if (isFetching.value && (!data.value || isPlaceholderData.value || isError.value)) {
    return 'loading';
  }
  if (isError.value) return 'error';
  return table.getRowModel().rows.length ? 'ready' : 'empty';
});

/** v9 の getIsSomePageRowsSelected は全選択でも true なので、全選択を除く */
const isSomePageRowsSelected = computed(
  () => table.getIsSomePageRowsSelected() && !table.getIsAllPageRowsSelected(),
);

const SEARCH_COLUMNS: DataListColumn[] = [
  { id: 'key', label: 'ID', width: '6rem' },
  { id: 'title', label: 'タイトル' },
  { id: 'highlight', label: '一致箇所' },
];

/** ラベルでの絞り込み。「すべて」は value を空にして選択なしに戻す */
const labelFilterItems = computed<DropdownItem[]>(() => [
  { value: '', label: 'すべて', kind: 'radio', checked: !selectedLabelId.value },
  ...projectLabels.value.map((label) => ({
    value: label.id,
    label: label.name,
    kind: 'radio' as const,
    checked: label.id === selectedLabelId.value,
  })),
]);

/** 外せる列の表示切り替え。checkbox なので選んでもメニューは閉じない */
const columnItems = computed<DropdownItem[]>(() =>
  table
    .getAllColumns()
    .filter((col) => col.getCanHide())
    .map((col) => ({
      value: col.id,
      label: col.id,
      kind: 'checkbox' as const,
      checked: col.getIsVisible(),
    })),
);
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col gap-3">
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

    <!-- 一覧（左）＋ 詳細ペイン（右）の分割ビュー -->
    <ResizablePanelGroup
      v-else-if="canCreate"
      direction="horizontal"
      auto-save-id="tasks-split-view"
      class="min-h-0 flex-1"
    >
      <ResizablePanel :order="1" :min-size="30" class="min-w-0">
        <div class="flex h-full min-h-0 flex-col gap-3">
          <!-- サーバー側検索ツールバー -->
          <div class="flex items-center gap-2">
            <form
              class="flex w-full max-w-md items-center gap-2"
              role="search"
              @submit.prevent="submitSearch"
            >
              <!-- 消去は suffix のボタンだけにする（ブラウザの検索欄の × は隠す） -->
              <Input
                type="search"
                class="min-w-0 flex-1 [&_input::-webkit-search-cancel-button]:hidden"
                placeholder="タスクを検索..."
                aria-label="タスクを検索"
                :value="searchInput"
                :on-value-change="scheduleSearch"
              >
                <template #prefix><Search class="size-4" /></template>
                <template #suffix>
                  <Button
                    v-if="searchInput"
                    type="button"
                    variant="ghost"
                    ariaLabel="検索をクリア"
                    @click="clearSearch"
                  >
                    <template #icon><X class="size-4" /></template>
                  </Button>
                </template>
              </Input>
              <Button
                type="submit"
                variant="tertiary"
                :disabled="!searchInput.trim()"
                :label="taskSearchQuery.isFetching.value && isSearchActive ? '検索中…' : '検索'"
              >
                <template #icon>
                  <Spinner v-if="taskSearchQuery.isFetching.value && isSearchActive" hide-label />
                  <Search v-else class="size-4" />
                </template>
              </Button>
            </form>
            <Button class="ml-auto" label="新規タスク" @click="isCreateDialogOpen = true" />
            <!-- ラベル取得失敗はタスク一覧をブロックせず、ツールバー内で再試行を出す -->
            <div
              v-if="!isSearchActive && labelsQuery.isError.value && !projectLabels.length"
              class="flex items-center gap-1.5 text-xs text-destructive"
            >
              <span>ラベルの取得に失敗しました</span>
              <Button variant="tertiary" label="再試行" @click="labelsQuery.refetch()" />
            </div>
            <Dropdown
              v-if="!isSearchActive && projectLabels.length"
              variant="tertiary"
              :class="selectedLabelId ? '[&>button]:text-link' : ''"
              :label="selectedLabelName ?? 'ラベル'"
              :items="labelFilterItems"
              :on-select="(value: string) => (selectedLabelId = value || null)"
            />
            <Dropdown
              v-if="!isSearchActive"
              variant="tertiary"
              label="列"
              :items="columnItems"
              :on-select="(id: string) => table.getColumn(id)?.toggleVisibility()"
            />
          </div>

          <!-- 表示形式の切り替え。既定は List -->
          <div class="flex items-center gap-1 border-b" role="tablist" aria-label="表示形式">
            <button
              v-for="option in VIEW_OPTIONS"
              :key="option.value"
              type="button"
              role="tab"
              :aria-selected="view === option.value"
              class="-mb-px flex items-center gap-1.5 border-b-2 px-2.5 py-1.5 text-sm transition-colors"
              :class="
                view === option.value
                  ? 'border-foreground font-semibold text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground'
              "
              @click="view = option.value"
            >
              <component :is="option.icon" class="size-4" />
              {{ option.label }}
            </button>
          </div>

          <CreateTaskDialog
            v-if="tenantId && projectId"
            v-model:open="isCreateDialogOpen"
            :tenant-id="tenantId"
            :project-id="projectId"
            :project-key="projectKey"
            :statuses="statusesQuery.data.value ?? []"
            :labels="labelsQuery.data.value"
            :labels-loading="labelsQuery.isLoading.value"
            :labels-error="labelsQuery.isError.value && !projectLabels.length"
            :members="membersQuery.data.value"
            :members-loading="membersQuery.isLoading.value"
            :members-error="membersQuery.isError.value && !projectMembers.length"
            @created="onTaskCreated"
            @retry-labels="labelsQuery.refetch()"
            @retry-members="membersQuery.refetch()"
          />

          <!--
            List 表示の取得。ステータスごとに 1 つの infinite query を持たせるため、
            描画しないコンポーネントを並べる。ページを 1 本ずつ別クエリにすると
            後続ページのキーにカーソルが焼き付き、取り直しのときに境界のタスクが落ちる。
            下の v-if の連なりには入れない（連鎖を切ると一覧が出なくなる）
          -->
          <TaskGroupQuery
            v-for="status in isListView ? workflowStatuses : []"
            :key="status.id"
            :status="status"
            :tenant-id="tenantId"
            :project-id="projectId"
            :label-id="selectedLabelId"
            :sort="taskGroupApiSort"
            :page-size="GROUP_PAGE_SIZE"
            :enabled="!isSearchActive"
            @update:group="setTaskGroup"
          />

          <!-- スクロールするテーブル領域（ツールバーとページネーションは固定） -->
          <div class="min-h-0 flex-1 overflow-y-auto">
            <!-- 検索結果。API は検索ヒットの最小情報のみ返すため、虚偽の状態値は補完しない。 -->
            <div
              v-if="
                isSearchActive && taskSearchQuery.isLoading.value && !taskSearchQuery.data.value
              "
              class="flex justify-center py-8"
            >
              <Loader2 class="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
            <div
              v-else-if="isSearchActive && taskSearchQuery.isError.value"
              class="flex flex-col items-center gap-2 py-8 text-sm text-destructive"
            >
              <span>検索に失敗しました</span>
              <Button variant="tertiary" label="再試行" @click="taskSearchQuery.refetch()" />
            </div>
            <template v-else-if="isSearchActive">
              <DataList
                id="task-search-results"
                label="検索結果"
                :heading-level="2"
                :columns="SEARCH_COLUMNS"
                :count="taskSearchQuery.data.value?.total ?? 0"
                :status="taskSearchQuery.data.value?.tasks.length ? 'ready' : 'empty'"
                message="検索結果がありません"
              >
                <DataListRow
                  v-for="task in taskSearchQuery.data.value?.tasks ?? []"
                  :key="task.id"
                  :class="isRowActive(task.seq_id) && 'bg-muted'"
                >
                  <td class="font-mono text-xs text-muted-foreground">
                    {{ projectKey }}-{{ task.seq_id }}
                  </td>
                  <td>
                    <TaskTitleLink
                      :tenant-display-id="tenantDisplayId"
                      :project-key="projectKey"
                      :seq-id="task.seq_id"
                      :title="task.title"
                      @select="onSelectRow"
                    />
                  </td>
                  <!-- highlight は backend の ilike/tsvector/no-match 全経路で動的文字列を
                     html_escape 済み。唯一 backend が付与する <em> のみ HTML として描画する。 -->
                  <td class="truncate text-xs text-muted-foreground" v-html="task.highlight" />
                </DataListRow>
              </DataList>
              <div class="text-xs text-muted-foreground">
                上位 {{ taskSearchQuery.data.value?.tasks.length ?? 0 }} 件 / 全
                {{ taskSearchQuery.data.value?.total ?? 0 }} 件
              </div>
            </template>

            <!-- List 表示: ステータスごとに並べる（既定） -->
            <TaskGroupedList
              v-else-if="isListView"
              :groups="taskGroups"
              :tenant-id="tenantId"
              :project-id="projectId"
              :label-id="selectedLabelId"
              :statuses="workflowStatuses"
              :project-labels="projectLabels"
              :members="projectMembers"
              :members-state="projectMembersState"
              :pending="rowMutations.pending.value"
              :errors="rowMutations.errors.value"
              :comment-pending-task-ids="rowMutations.commentPendingTaskIds.value"
              :create-errors="rowMutations.createErrors.value"
              :sorting="sorting"
              @open="openOverlay"
              @more="loadMoreInGroup"
              @update:status="(task, statusId) => rowMutations.setStatus(task, statusId)"
              @update:priority="(task, priority) => rowMutations.setPriority(task, priority)"
              @update:soft-deadline="(task, iso) => rowMutations.setSoftDeadline(task, iso)"
              @toggle:assignee="
                (task, userId, checked) => rowMutations.toggleAssignee(task, userId, checked)
              "
              @toggle:label="
                (task, labelId, checked) => rowMutations.toggleLabel(task, labelId, checked)
              "
              @update:sorting="onListSortingChange"
              :on-comment="(task, body) => rowMutations.addComment(task.id, body)"
              :creating-status-ids="rowMutations.creatingStatusIds.value"
              :on-create="rowMutations.createTask"
              :on-move-status="(task, statusId) => rowMutations.setStatus(task, statusId)"
            />

            <!-- 通常一覧テーブル。行はクリック操作を持たず、開くのはタイトルのリンク -->
            <DataList
              v-else
              id="task-table"
              label="タスク一覧"
              class="[--koyori-data-list-min-width:64rem]"
              :heading-level="2"
              :columns="taskListColumns"
              :count="fetchedTaskTotal ?? undefined"
              :status="taskListStatus"
              :message="taskListStatus === 'empty' ? 'タスクが見つかりません' : undefined"
              :on-retry="() => void tasksQuery.refetch()"
              :sort="taskListSort"
              :on-sort-change="onTaskListSortChange"
            >
              <DataListRow
                v-for="row in table.getRowModel().rows"
                :key="row.id"
                :selected="row.getIsSelected()"
                :class="isRowActive(row.original.seq_id) && 'bg-muted'"
              >
                <td v-for="cell in row.getVisibleCells()" :key="cell.id">
                  <FlexRender :render="cell.column.columnDef.cell" :props="cell.getContext()" />
                </td>
              </DataListRow>
            </DataList>
          </div>

          <!-- ページネーション（API total 連動のサーバーサイド）。
               List 表示はステータスごとに「もっと見る」で伸ばすのでページャは出さない -->
          <div
            v-if="!isSearchActive && !isListView"
            class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
          >
            <div class="flex items-center gap-2">
              <Checkbox
                label="すべて選択"
                hide-label
                :checked="table.getIsAllPageRowsSelected()"
                :indeterminate="isSomePageRowsSelected"
                :on-checked-change="(checked: boolean) => table.toggleAllPageRowsSelected(checked)"
              />
              <span>
                {{ table.getFilteredSelectedRowModel().rows.length }} / {{ taskTotal }} 件選択
              </span>
            </div>
            <Pagination
              label="タスクのページ"
              :page="pagination.pageIndex + 1"
              :page-size="pagination.pageSize"
              :total="fetchedTaskTotal ?? undefined"
              :loading="taskListStatus === 'loading'"
              :on-page-change="(page: number) => table.setPageIndex(page - 1)"
            />
          </div>
        </div>
      </ResizablePanel>

      <template v-if="showDetail">
        <ResizableHandle with-handle />
        <ResizablePanel :order="2" :default-size="40" :min-size="26" class="min-w-0">
          <TaskDetailPane
            :key="selectedTaskId ?? ''"
            :tenant-display-id="tenantDisplayId"
            :project-key="projectKey"
            :task-id="selectedTaskId ?? ''"
            @close="closeDetail"
            @open-task="openOverlay"
            @open-task-ref="openTaskRefInPane"
          />
        </ResizablePanel>
      </template>
    </ResizablePanelGroup>

    <!-- List 表示の詳細は分割ではなくオーバーレイ -->
    <TaskDetailOverlay
      v-if="overlayRenderedTaskSeqKey"
      :key="overlayRenderedTaskSeqKey"
      :open="!!overlayTaskSeqKey"
      :tenant-display-id="tenantDisplayId"
      :project-key="projectKey"
      :task-id="overlayRenderedTaskSeqKey"
      @open-task="openTaskFromOverlay"
      @open-task-ref="openTaskRefFromOverlay"
      @update:open="onOverlayOpenChange"
    />
  </div>
</template>

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defineComponent, ref, type MaybeRefOrGetter } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import type { paths } from '@/generated/api';
import { useTaskDetail } from '../useTaskDetail';

const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const TASK_SEARCH_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search' as const;
const TASK_RELATIONS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/relations' as const;

// vi.mock の factory から参照するため hoisted に置く
const {
  TENANT_ID,
  PROJECT_ID,
  TASK_SEQ_KEY,
  OTHER_TASK_SEQ_KEY,
  baseTask,
  otherTask,
  putControl,
  labelsControl,
  fetchMock,
} = vi.hoisted(() => {
  const TENANT_ID = 'tenant-1';
  const PROJECT_ID = 'project-1';
  const TASK_SEQ_KEY = 'ENG-1';
  const OTHER_TASK_SEQ_KEY = 'ENG-2';

  const baseTask = {
    id: '00000000-0000-0000-0000-000000000010',
    seq_key: TASK_SEQ_KEY,
    title: '元のタイトル',
    description: null,
    status_id: 'status-1',
    priority: 'Medium',
    progress_pct: 0,
    soft_deadline: null,
    hard_deadline: null,
    // テストごとに差し替えるため、空配列（never[]）に推論させない
    labels: [] as Record<string, unknown>[],
    parent_task_id: null as string | null,
  };

  // 移動先のタスク。status_id と priority は baseTask と同じ
  const otherTask = {
    ...baseTask,
    id: '00000000-0000-0000-0000-000000000011',
    seq_key: OTHER_TASK_SEQ_KEY,
    title: '別のタスク',
  };

  function jsonResponse(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  // PUT を保留して任意のタイミングで完了（成功/失敗）させるための deferred
  const putControl: {
    resolve?: (task: Record<string, unknown>) => void;
    fail?: (status?: number) => void;
    body?: Promise<unknown>;
  } = {};

  // ラベル一覧 GET の応答をテストごとに切り替える
  const labelsControl: { mode: 'success' | 'error' | 'pending'; data: unknown[] } = {
    mode: 'success',
    data: [],
  };

  const fetchMock = async (input: Request) => {
    const url = input.url;
    const method = input.method.toUpperCase();

    if (method === 'GET' && url.endsWith(`/tasks/${TASK_SEQ_KEY}`)) {
      return jsonResponse(baseTask);
    }
    if (method === 'GET' && url.endsWith(`/tasks/${OTHER_TASK_SEQ_KEY}`)) {
      return jsonResponse(otherTask);
    }
    if (method === 'GET' && url.endsWith('/statuses')) {
      return jsonResponse([]);
    }
    if (method === 'GET' && url.endsWith('/labels')) {
      if (labelsControl.mode === 'pending') return new Promise<Response>(() => {});
      if (labelsControl.mode === 'error') return jsonResponse({ message: 'boom' }, 500);
      return jsonResponse(labelsControl.data);
    }
    if (
      method === 'PUT' &&
      (url.endsWith(`/tasks/${TASK_SEQ_KEY}`) || url.endsWith(`/tasks/${OTHER_TASK_SEQ_KEY}`))
    ) {
      // await すると putControl.resolve の代入が遅れるので、Promise のまま持つ
      putControl.body = input.clone().json();
      return new Promise<Response>((resolve) => {
        putControl.resolve = (task) => resolve(jsonResponse(task));
        putControl.fail = (status = 400) => resolve(jsonResponse({ message: 'invalid' }, status));
      });
    }
    if (method === 'DELETE' && url.endsWith(`/tasks/${TASK_SEQ_KEY}`)) {
      return new Response(null, { status: 204 });
    }
    return jsonResponse({ message: 'not found' }, 404);
  };

  return {
    TENANT_ID,
    PROJECT_ID,
    TASK_SEQ_KEY,
    OTHER_TASK_SEQ_KEY,
    baseTask,
    otherTask,
    putControl,
    labelsControl,
    fetchMock,
  };
});

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  const { default: createFetchClient } = await import('openapi-fetch');
  const { createClient } = await import('@koyori-app/openapi-vue-query');
  const testFetchClient = createFetchClient<paths>({
    baseUrl: 'http://test.local/api',
    fetch: (req: Request) => fetchMock(req),
  });
  return {
    ...actual,
    fetchClient: testFetchClient,
    apiClient: createClient<paths>(testFetchClient),
  };
});

vi.mock('@/composables/useResolvedTenantId', () => ({
  useResolvedTenantId: () => ({
    tenantId: ref(TENANT_ID),
    isTenantNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
  }),
}));

vi.mock('@/composables/useResolvedProjectId', () => ({
  useResolvedProjectId: () => ({
    projectId: ref(PROJECT_ID),
    isProjectNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
  }),
}));

const taskQueryKey = [
  'get',
  GET_TASK_PATH,
  { params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id: TASK_SEQ_KEY } } },
] as const;
const otherTaskQueryKey = [
  'get',
  GET_TASK_PATH,
  { params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id: OTHER_TASK_SEQ_KEY } } },
] as const;
const listQueryKey = [
  'get',
  LIST_TASKS_PATH,
  { params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID } } },
] as const;
// カレンダーの帯。オーバーレイで詳細を開いたまま裏に残るので、更新・削除で取り直す
const scheduleQueryKey = [
  'get',
  '/v1/tenants/{tenant_id}/schedule',
  { params: { path: { tenant_id: TENANT_ID }, query: { from: '2026-10-12', to: '2026-10-25' } } },
] as const;
const searchQueryKey = [
  'get',
  TASK_SEARCH_PATH,
  {
    params: {
      path: { tenant_id: TENANT_ID, project_id: PROJECT_ID },
      query: { q: '元の' },
    },
  },
] as const;

describe('useTaskDetail のキャッシュ同期', () => {
  let queryClient: QueryClient;
  let detail: ReturnType<typeof useTaskDetail>;
  let onAfterDelete: ReturnType<typeof vi.fn<(listHref: string) => void>>;

  function mountHost(taskId: MaybeRefOrGetter<string> = TASK_SEQ_KEY) {
    const Host = defineComponent({
      setup() {
        detail = useTaskDetail({
          tenantDisplayId: 'acme',
          projectKey: 'ENG',
          taskId,
          onAfterDelete,
        });
        return () => null;
      },
    });
    return mount(Host, {
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    });
  }

  function seedListAndSearchCaches() {
    queryClient.setQueryData(listQueryKey, { tasks: [baseTask] });
    queryClient.setQueryData(searchQueryKey, { tasks: [baseTask] });
    queryClient.setQueryData(scheduleQueryKey, { tasks: [] });
  }

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    onAfterDelete = vi.fn<(listHref: string) => void>();
    putControl.resolve = undefined;
    putControl.body = undefined;
    labelsControl.mode = 'success';
    labelsControl.data = [];
    baseTask.labels = [];
    baseTask.parent_task_id = null;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('ラベル一覧の取得失敗は projectLabelsError として公開し、詳細全体の isError にはしない', async () => {
    labelsControl.mode = 'error';
    mountHost();
    await flushPromises();

    expect(detail.projectLabelsError.value).toBe(true);
    expect(detail.projectLabelsLoading.value).toBe(false);
    expect(detail.projectLabels.value).toEqual([]);
    expect(detail.isError.value).toBe(false);
    expect(detail.isLoading.value).toBe(false);
  });

  it('ラベル一覧の取得中は projectLabelsLoading が true になる', async () => {
    labelsControl.mode = 'pending';
    mountHost();
    await flushPromises();

    expect(detail.projectLabelsLoading.value).toBe(true);
    expect(detail.projectLabelsError.value).toBe(false);
    expect(detail.isLoading.value).toBe(false);
  });

  it('ペイン切替（unmount）後に完了した更新でも、詳細キャッシュ更新と一覧・検索の invalidate が走る', async () => {
    const wrapper = mountHost();
    await flushPromises();
    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({ title: '元のタイトル' });

    seedListAndSearchCaches();

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    expect(putControl.resolve).toBeDefined();

    // PUT が完了する前にペインを切り替える（コンポーネント再生成で unmount）
    wrapper.unmount();

    putControl.resolve!({ ...baseTask, title: '新しいタイトル' });
    await flushPromises();

    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({ title: '新しいタイトル' });
    expect(queryClient.getQueryState(listQueryKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(searchQueryKey)?.isInvalidated).toBe(true);
  });

  it('状態を変えたら、応答の completed_at も詳細のキャッシュへ入る', async () => {
    mountHost();
    await flushPromises();

    detail.onStatusChange('status-done');
    await flushPromises();
    putControl.resolve!({
      ...baseTask,
      status_id: 'status-done',
      completed_at: '2026-07-14T09:00:00Z',
    });
    await flushPromises();

    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({
      status_id: 'status-done',
      completed_at: '2026-07-14T09:00:00Z',
    });
  });

  it('状態以外の更新では、completed_at を応答で書き換えない', async () => {
    mountHost();
    await flushPromises();
    queryClient.setQueryData(taskQueryKey, {
      ...baseTask,
      completed_at: '2026-07-14T09:00:00Z',
    });

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    putControl.resolve!({ ...baseTask, title: '新しいタイトル', completed_at: null });
    await flushPromises();

    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({
      title: '新しいタイトル',
      completed_at: '2026-07-14T09:00:00Z',
    });
  });

  it('検索結果表示中の更新で検索キャッシュとカレンダーの予定も invalidate される（mount したまま）', async () => {
    mountHost();
    await flushPromises();
    seedListAndSearchCaches();

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    putControl.resolve!({ ...baseTask, title: '新しいタイトル' });
    await flushPromises();

    expect(queryClient.getQueryState(searchQueryKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(listQueryKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(scheduleQueryKey)?.isInvalidated).toBe(true);
  });

  it('保存が確定したら savedField にそのフィールドが入り、2 秒で消える', async () => {
    // vue-query の notifyManager は setTimeout で通知するので、fake timer 中は
    // flushPromises だけでなく advanceTimersByTimeAsync でも進める
    vi.useFakeTimers();
    mountHost();
    await flushPromises();
    await vi.advanceTimersByTimeAsync(0);

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    await vi.advanceTimersByTimeAsync(0);
    // 飛行中はまだ保存していない
    expect(detail.savedField.value).toBeNull();

    putControl.resolve!({ ...baseTask, title: '新しいタイトル' });
    await flushPromises();
    await vi.advanceTimersByTimeAsync(0);
    expect(detail.savedField.value).toBe('title');

    await vi.advanceTimersByTimeAsync(1_999);
    expect(detail.savedField.value).toBe('title');
    await vi.advanceTimersByTimeAsync(1);
    expect(detail.savedField.value).toBeNull();
  });

  it('通知の欄が無い優先度の保存では、表示中の savedField を置き換えない', async () => {
    mountHost();
    await flushPromises();

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    putControl.resolve!({ ...baseTask, title: '新しいタイトル' });
    await flushPromises();
    expect(detail.savedField.value).toBe('title');

    detail.onPriorityChange('High');
    await flushPromises();
    putControl.resolve!({ ...baseTask, title: '新しいタイトル', priority: 'High' });
    await flushPromises();

    expect(detail.savedField.value).toBe('title');
  });

  it('別のタスクへ移ったら、前のタスクの savedField を消す', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    putControl.resolve!({ ...baseTask, title: '新しいタイトル' });
    await flushPromises();
    expect(detail.savedField.value).toBe('title');

    taskId.value = 'ENG-2';
    await flushPromises();
    expect(detail.savedField.value).toBeNull();
  });

  it('移る前に始めた保存が移った後に完了しても、移った先のタスクに savedField を立てない', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    taskId.value = 'ENG-2';
    await flushPromises();
    putControl.resolve!({ ...baseTask, title: '新しいタイトル' });
    await flushPromises();

    expect(detail.savedField.value).toBeNull();
    // 保存そのものは移る前のタスクのキャッシュへ反映される
    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({ title: '新しいタイトル' });
  });

  it('楽観更新のあと別のタスクへ移ると、displayTask と進行中フラグに前のタスクの値が残らない', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();

    detail.onSaveTitle('新しいタイトル');
    detail.onPriorityChange('High');
    await flushPromises();
    expect(detail.displayTask.value).toMatchObject({ title: '新しいタイトル', priority: 'High' });
    expect(detail.fieldUpdating.value.title).toBe(true);
    expect(detail.priorityUpdating.value).toBe(true);

    taskId.value = OTHER_TASK_SEQ_KEY;
    await flushPromises();

    expect(detail.displayTask.value).toMatchObject({
      seq_key: OTHER_TASK_SEQ_KEY,
      title: otherTask.title,
      priority: otherTask.priority,
    });
    expect(detail.fieldUpdating.value.title).toBe(false);
    expect(detail.priorityUpdating.value).toBe(false);
  });

  it('同じステータスのタスクへ移っても、前のタスクで選んだステータスを残さない', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();
    // 移動先がキャッシュ済みだと status_id は変化せず、status_id の監視だけでは戻らない
    queryClient.setQueryData(otherTaskQueryKey, otherTask);

    detail.onStatusChange('status-2');
    await flushPromises();
    expect(detail.selectedStatusId.value).toBe('status-2');

    taskId.value = OTHER_TASK_SEQ_KEY;
    await flushPromises();

    expect(detail.selectedStatusId.value).toBe(otherTask.status_id);
  });

  it('移る前に始めた保存が移った後に完了しても、移った先のステータスを書き換えない', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();

    detail.onStatusChange('status-2');
    await flushPromises();
    taskId.value = OTHER_TASK_SEQ_KEY;
    await flushPromises();
    putControl.resolve!({ ...baseTask, status_id: 'status-2' });
    await flushPromises();

    expect(detail.selectedStatusId.value).toBe(otherTask.status_id);
    expect(detail.displayTask.value?.status_id).toBe(otherTask.status_id);
    // 保存そのものは移る前のタスクのキャッシュへ反映される
    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({ status_id: 'status-2' });
  });

  it('移る前に始めた保存が移った後に失敗しても、移った先にエラーを出さない', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    taskId.value = OTHER_TASK_SEQ_KEY;
    await flushPromises();
    putControl.fail!(500);
    await flushPromises();

    expect(detail.fieldErrors.value.title).toBeUndefined();
  });

  it('移った先の保存が先に完了しても、移る前のタスクの保存をキャッシュへ反映する', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();

    detail.onSaveTitle('前のタスクの新タイトル');
    await flushPromises();
    const resolveFirst = putControl.resolve!;
    taskId.value = OTHER_TASK_SEQ_KEY;
    await flushPromises();

    detail.onSaveTitle('移った先の新タイトル');
    await flushPromises();
    putControl.resolve!({ ...otherTask, title: '移った先の新タイトル' });
    await flushPromises();
    resolveFirst({ ...baseTask, title: '前のタスクの新タイトル' });
    await flushPromises();

    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({
      title: '前のタスクの新タイトル',
    });
    expect(queryClient.getQueryData(otherTaskQueryKey)).toMatchObject({
      title: '移った先の新タイトル',
    });
  });

  it('同じタスクの新しい保存が先に完了していれば、移った後に届いた古い保存でキャッシュを巻き戻さない', async () => {
    const taskId = ref(TASK_SEQ_KEY);
    mountHost(taskId);
    await flushPromises();

    detail.onSaveTitle('1 回目');
    await flushPromises();
    const resolveFirst = putControl.resolve!;
    detail.onSaveTitle('2 回目');
    await flushPromises();
    putControl.resolve!({ ...baseTask, title: '2 回目' });
    await flushPromises();

    taskId.value = OTHER_TASK_SEQ_KEY;
    await flushPromises();
    resolveFirst({ ...baseTask, title: '1 回目' });
    await flushPromises();

    expect(queryClient.getQueryData(taskQueryKey)).toMatchObject({ title: '2 回目' });
  });

  it('保存に失敗したら savedField は立てず、エラーを出す', async () => {
    mountHost();
    await flushPromises();

    detail.onSaveTitle('新しいタイトル');
    await flushPromises();
    putControl.fail!(500);
    await flushPromises();

    expect(detail.savedField.value).toBeNull();
    expect(detail.fieldErrors.value.title).toBe('更新に失敗しました');
  });

  it('ラベル更新の楽観値を名前順、同名時は ID 順に並べる', async () => {
    const alpha1 = {
      id: '00000000-0000-0000-0000-000000000001',
      name: 'alpha',
      description: '',
      color: '#111111',
      icon_url: null,
      project_id: PROJECT_ID,
    };
    const alpha2 = { ...alpha1, id: '00000000-0000-0000-0000-000000000002' };
    const beta = {
      ...alpha1,
      id: '00000000-0000-0000-0000-000000000003',
      name: 'beta',
    };
    labelsControl.data = [beta, alpha2, alpha1];
    mountHost();
    await vi.waitFor(() => {
      expect(detail.projectLabels.value).toEqual([beta, alpha2, alpha1]);
    });

    detail.onSaveLabels([beta.id, alpha2.id, alpha1.id]);
    await vi.waitFor(() => {
      expect(detail.displayTask.value?.labels).toEqual([alpha1, alpha2, beta]);
    });

    putControl.resolve!({ ...baseTask, labels: [alpha1, alpha2, beta] });
    await flushPromises();
  });

  it('ラベル一覧が古くて欠けていても、既にタスクに付いているラベルは楽観値に残す', async () => {
    const kept = {
      id: '00000000-0000-0000-0000-000000000001',
      name: 'kept',
      description: '',
      color: '#111111',
      icon_url: null,
      project_id: PROJECT_ID,
    };
    const added = { ...kept, id: '00000000-0000-0000-0000-000000000002', name: 'added' };

    // タスクには kept が付いているが、独立キャッシュのラベル一覧は古くて kept を含まない。
    // 楽観値を一覧だけから作ると、kept が無関係に一瞬消えて見える
    baseTask.labels = [kept];
    labelsControl.data = [added];
    mountHost();
    await vi.waitFor(() => {
      expect(detail.projectLabels.value).toEqual([added]);
    });

    detail.onSaveLabels([kept.id, added.id]);
    await vi.waitFor(() => {
      expect(detail.displayTask.value?.labels).toEqual([added, kept]);
    });

    putControl.resolve!({ ...baseTask, labels: [added, kept] });
    await flushPromises();
  });

  it('ラベル名の並びはコードポイント順（サーバの Rust 側と一致し、UTF-16 順にしない）', async () => {
    const base = {
      description: '',
      color: '#111111',
      icon_url: null,
      project_id: PROJECT_ID,
    };
    // U+1F41B（補助面、UTF-16 ではサロゲートペア 0xD83D…）と U+FF8A（BMP 後方）。
    // UTF-16 コードユニット順だと 🐛bug が先、コードポイント順だと ﾊﾞｸﾞ が先になる
    const emoji = { ...base, id: '00000000-0000-0000-0000-000000000001', name: '🐛bug' };
    const halfKana = { ...base, id: '00000000-0000-0000-0000-000000000002', name: 'ﾊﾞｸﾞ' };
    labelsControl.data = [emoji, halfKana];
    mountHost();
    await vi.waitFor(() => {
      expect(detail.projectLabels.value).toEqual([emoji, halfKana]);
    });

    detail.onSaveLabels([emoji.id, halfKana.id]);
    await vi.waitFor(() => {
      expect(detail.displayTask.value?.labels).toEqual([halfKana, emoji]);
    });

    putControl.resolve!({ ...baseTask, labels: [halfKana, emoji] });
    await flushPromises();
  });

  it('ラベル保存が 400 で失敗したら楽観値を巻き戻し、タスクとラベル一覧を再取得する', async () => {
    const bug = {
      id: '00000000-0000-0000-0000-000000000001',
      name: 'bug',
      description: '',
      color: '#111111',
      icon_url: null,
      project_id: PROJECT_ID,
    };
    labelsControl.data = [bug];
    mountHost();
    await flushPromises();
    const labelsQueryKey = [
      'get',
      '/v1/tenants/{tenant_id}/projects/{project_id}/labels',
      { params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID } } },
    ] as const;
    expect(queryClient.getQueryState(labelsQueryKey)).toBeDefined();

    detail.onSaveLabels([bug.id]);
    await flushPromises();
    expect(detail.displayTask.value?.labels).toEqual([bug]);
    const taskUpdates = queryClient.getQueryState(taskQueryKey)!.dataUpdateCount;
    const labelsUpdates = queryClient.getQueryState(labelsQueryKey)!.dataUpdateCount;

    putControl.fail!(400);
    await flushPromises();

    expect(detail.displayTask.value?.labels).toEqual([]);
    expect(detail.labelsError.value).toBe('ラベルの更新に失敗しました');
    // invalidate による再取得が完走していること（observer があるので即 refetch される）
    expect(queryClient.getQueryState(taskQueryKey)!.dataUpdateCount).toBeGreaterThan(taskUpdates);
    expect(queryClient.getQueryState(labelsQueryKey)!.dataUpdateCount).toBeGreaterThan(
      labelsUpdates,
    );
  });

  it('優先度の更新が 400 で失敗したら楽観値を巻き戻し、エラーを立てる', async () => {
    mountHost();
    await flushPromises();

    detail.onPriorityChange('High');
    await flushPromises();
    // 飛行中は楽観値が出ていて、二度目の選択を止めるフラグが立っている
    expect(detail.displayTask.value?.priority).toBe('High');
    expect(detail.priorityUpdating.value).toBe(true);

    putControl.fail!(400);
    await flushPromises();

    // 巻き戻さないと、失敗したのに変わったように見えたまま残る
    expect(detail.displayTask.value?.priority).toBe('Medium');
    expect(detail.priorityError.value).toBe('優先度の更新に失敗しました');
    expect(detail.priorityUpdating.value).toBe(false);
  });

  describe('親タスクの変更', () => {
    function relationsKey(id: string) {
      return [
        'get',
        TASK_RELATIONS_PATH,
        { params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id } } },
      ] as const;
    }

    it('親が無いタスクへの設定は parent_task_id を送り、確定まで楽観値と進行中フラグを出す', async () => {
      mountHost();
      await flushPromises();

      detail.onChangeParent('parent-new');
      await flushPromises();

      expect(await putControl.body).toEqual({ parent_task_id: 'parent-new' });
      expect(detail.displayTask.value?.parent_task_id).toBe('parent-new');
      expect(detail.parentUpdating.value).toBe(true);

      baseTask.parent_task_id = 'parent-new';
      putControl.resolve!({ ...baseTask });
      await flushPromises();

      expect(detail.parentUpdating.value).toBe(false);
      expect(detail.displayTask.value?.parent_task_id).toBe('parent-new');
      expect(detail.parentError.value).toBeNull();
    });

    it('親の変更は新しい parent_task_id を送り、元と新しい親の関連・一覧・本体を取り直す', async () => {
      baseTask.parent_task_id = 'parent-old';
      mountHost();
      await flushPromises();
      seedListAndSearchCaches();
      const oldParentDetailKey = [
        'get',
        GET_TASK_PATH,
        { params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id: 'ENG-9' } } },
      ] as const;
      const parentSubtasksKey = [...listQueryKey, 'subtasks'] as const;
      queryClient.setQueryData(relationsKey('parent-old'), { parent: null, subtasks: [] });
      queryClient.setQueryData(relationsKey('parent-new'), { parent: null, subtasks: [] });
      queryClient.setQueryData(oldParentDetailKey, { ...otherTask });
      queryClient.setQueryData(parentSubtasksKey, []);
      const taskUpdates = queryClient.getQueryState(taskQueryKey)!.dataUpdateCount;

      detail.onChangeParent('parent-new');
      await flushPromises();
      expect(await putControl.body).toEqual({ parent_task_id: 'parent-new' });

      baseTask.parent_task_id = 'parent-new';
      putControl.resolve!({ ...baseTask });
      await flushPromises();

      expect(queryClient.getQueryState(relationsKey('parent-old'))?.isInvalidated).toBe(true);
      expect(queryClient.getQueryState(relationsKey('parent-new'))?.isInvalidated).toBe(true);
      expect(queryClient.getQueryState(parentSubtasksKey)?.isInvalidated).toBe(true);
      expect(queryClient.getQueryState(oldParentDetailKey)?.isInvalidated).toBe(true);
      expect(queryClient.getQueryState(listQueryKey)?.isInvalidated).toBe(true);
      expect(queryClient.getQueryState(searchQueryKey)?.isInvalidated).toBe(true);
      // 開いているタスク本体は observer があるので invalidate がそのまま再取得になる
      // （応答の書き込みと再取得で 2 回以上進む）
      expect(queryClient.getQueryState(taskQueryKey)!.dataUpdateCount).toBeGreaterThan(
        taskUpdates + 1,
      );
    });

    it('親以外の保存ではタスク本体を取り直さない', async () => {
      mountHost();
      await flushPromises();
      const taskUpdates = queryClient.getQueryState(taskQueryKey)!.dataUpdateCount;

      detail.onSaveTitle('新しいタイトル');
      await flushPromises();
      putControl.resolve!({ ...baseTask, title: '新しいタイトル' });
      await flushPromises();

      expect(queryClient.getQueryState(taskQueryKey)!.dataUpdateCount).toBe(taskUpdates + 1);
    });

    it('親の解除は clear_parent_task_id: true を送る', async () => {
      baseTask.parent_task_id = 'parent-old';
      mountHost();
      await flushPromises();

      detail.onChangeParent(null);
      await flushPromises();

      expect(await putControl.body).toEqual({ clear_parent_task_id: true });
      expect(detail.displayTask.value?.parent_task_id).toBeNull();

      baseTask.parent_task_id = null;
      putControl.resolve!({ ...baseTask });
      await flushPromises();
      expect(detail.displayTask.value?.parent_task_id).toBeNull();
    });

    it('今と同じ親を選んでも、親が無いタスクを解除しても送らない', async () => {
      mountHost();
      await flushPromises();
      detail.onChangeParent(null);
      await flushPromises();
      expect(putControl.body).toBeUndefined();
      expect(detail.parentUpdating.value).toBe(false);
    });

    it.each([
      [409, '自分自身や子孫のタスクは親にできません'],
      [404, 'タスクが見つかりません。削除された可能性があります'],
      [500, '親タスクの更新に失敗しました'],
    ])('%i で拒まれたら親を巻き戻し、理由を出す', async (status, message) => {
      baseTask.parent_task_id = 'parent-old';
      mountHost();
      await flushPromises();

      detail.onChangeParent('descendant');
      await flushPromises();
      expect(detail.displayTask.value?.parent_task_id).toBe('descendant');

      putControl.fail!(status);
      await flushPromises();

      expect(detail.displayTask.value?.parent_task_id).toBe('parent-old');
      expect(detail.parentError.value).toBe(message);
      expect(detail.parentUpdating.value).toBe(false);
    });

    it('次の変更を始めたら前のエラーを消す', async () => {
      mountHost();
      await flushPromises();
      detail.onChangeParent('descendant');
      await flushPromises();
      putControl.fail!(409);
      await flushPromises();
      expect(detail.parentError.value).not.toBeNull();

      detail.onChangeParent('parent-new');
      await flushPromises();
      expect(detail.parentError.value).toBeNull();
    });

    it('別のタスクへ移ったら、前のタスクの親のエラーを残さない', async () => {
      const taskId = ref(TASK_SEQ_KEY);
      mountHost(taskId);
      await flushPromises();
      detail.onChangeParent('descendant');
      await flushPromises();
      putControl.fail!(409);
      await flushPromises();
      expect(detail.parentError.value).not.toBeNull();

      taskId.value = OTHER_TASK_SEQ_KEY;
      await flushPromises();
      expect(detail.parentError.value).toBeNull();
    });
  });

  it('削除成功時に検索キャッシュとカレンダーの予定が invalidate され、詳細キャッシュは除去される', async () => {
    mountHost();
    await flushPromises();
    seedListAndSearchCaches();

    detail.confirmDelete();
    await flushPromises();

    expect(queryClient.getQueryState(taskQueryKey)).toBeUndefined();
    expect(queryClient.getQueryState(listQueryKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(searchQueryKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(scheduleQueryKey)?.isInvalidated).toBe(true);
    expect(onAfterDelete).toHaveBeenCalledWith('/acme/projects/ENG/tasks');
  });
});

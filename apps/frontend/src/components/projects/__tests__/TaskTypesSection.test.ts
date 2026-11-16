import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, DOMWrapper, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient, useQuery } from '@tanstack/vue-query';

const {
  createMutateAsync,
  updateMutateAsync,
  deleteMutateAsync,
  reorderMutateAsync,
  queryState,
  mutationPending,
} = vi.hoisted(() => ({
  createMutateAsync: vi.fn(),
  updateMutateAsync: vi.fn(),
  deleteMutateAsync: vi.fn(),
  reorderMutateAsync: vi.fn(),
  queryState: {
    taskTypes: [] as unknown[],
    isPending: false,
    isError: false,
    isFetching: false,
  },
  // vi.hoisted は import 前に走るため vue の ref は使えない。
  // コンポーネントは .value を読むだけなので素のオブジェクトで足りる
  mutationPending: {
    post: { value: false },
    put: { value: false },
    delete: { value: false },
  },
}));

// TaskTypesSection は共有ヘルパー projectTaskTypesQueryOptions を素の useQuery で消費するため、
// query は @tanstack/vue-query の useQuery をモックする（ヘルパー自体は実物 pass-through）。
vi.mock('@tanstack/vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tanstack/vue-query')>();
  return {
    ...actual,
    useQuery: vi.fn(() => ({
      data: { value: queryState.taskTypes },
      isPending: { value: queryState.isPending },
      isFetching: { value: queryState.isFetching },
      isError: { value: queryState.isError },
    })),
  };
});

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn((method: string, path: string) => ({
        mutateAsync:
          method === 'post'
            ? createMutateAsync
            : method === 'put'
              ? path.endsWith('/reorder')
                ? reorderMutateAsync
                : updateMutateAsync
              : deleteMutateAsync,
        isPending:
          method === 'post'
            ? mutationPending.post
            : method === 'put'
              ? mutationPending.put
              : mutationPending.delete,
      })),
    },
  };
});

import TaskTypesSection from '../TaskTypesSection.vue';
import type { components } from '@/generated/api';

type TaskTypeResponse = components['schemas']['TaskTypeResponse'];

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const bugType: TaskTypeResponse = {
  id: '00000000-0000-4000-8000-000000000021',
  name: 'バグ',
  color: '#ef4444',
  position: 0,
  project_id: PROJECT_UUID,
};

const featureType: TaskTypeResponse = {
  id: '00000000-0000-4000-8000-000000000022',
  name: '機能',
  color: '#3b82f6',
  position: 1,
  project_id: PROJECT_UUID,
};

let queryClient: QueryClient;

function mountView() {
  queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(TaskTypesSection, {
    props: { tenantId: TENANT_UUID, projectId: PROJECT_UUID },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function input(id: string) {
  const el = document.body.querySelector<HTMLInputElement | HTMLTextAreaElement>(`#${id}`);
  if (!el) throw new Error(`input #${id} not found`);
  return new DOMWrapper(el);
}

function formEl() {
  const el = document.body.querySelector('form');
  if (!el) throw new Error('form not found');
  return new DOMWrapper(el);
}

function clickBodyButton(label: string) {
  const button = [...document.body.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!button) throw new Error(`button "${label}" not found`);
  button.click();
}

/** 開いているダイアログ。閉じたダイアログも DOM に残るので open で絞る */
function openDialog() {
  return document.body.querySelector<HTMLDialogElement>('dialog[open]');
}

/** Esc（dialog の cancel）を直接シミュレート */
function pressEscape() {
  const dialog = openDialog();
  if (!dialog) throw new Error('open dialog not found');
  dialog.dispatchEvent(new Event('cancel', { cancelable: true }));
}

function clickByAriaLabel(label: string) {
  const button = document.body.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
  if (!button) throw new Error(`button[aria-label="${label}"] not found`);
  button.click();
}

describe('TaskTypesSection', () => {
  beforeEach(() => {
    createMutateAsync.mockReset();
    updateMutateAsync.mockReset();
    deleteMutateAsync.mockReset();
    reorderMutateAsync.mockReset();
    queryState.taskTypes = [];
    queryState.isPending = false;
    queryState.isError = false;
    queryState.isFetching = false;
    mutationPending.post.value = false;
    mutationPending.put.value = false;
    mutationPending.delete.value = false;
    document.body.innerHTML = '';
  });

  it('種別を並び順のまま行形式で一覧表示する', async () => {
    queryState.taskTypes = [bugType, featureType];
    mountView();
    await flushPromises();

    const rows = [...document.body.querySelectorAll('li')].map((li) => li.textContent?.trim());
    expect(rows[0]).toContain('バグ');
    expect(rows[1]).toContain('機能');
    expect(document.body.querySelector('button[aria-label="種別「バグ」を編集"]')).not.toBeNull();
    expect(document.body.querySelector('button[aria-label="種別「バグ」を削除"]')).not.toBeNull();
  });

  it('種別が 0 件なら空状態を表示する', async () => {
    mountView();
    await flushPromises();

    expect(document.body.textContent).toContain('種別はまだありません');
  });

  it('読み込みの失敗を空状態と区別して出す', async () => {
    queryState.isError = true;
    mountView();
    await flushPromises();

    expect(document.body.textContent).toContain('種別を読み込めませんでした');
    expect(document.body.textContent).not.toContain('種別はまだありません');
  });

  it('作成ダイアログから POST を送り、成功でダイアログを閉じる', async () => {
    createMutateAsync.mockResolvedValue({ ...bugType, name: '調査' });
    mountView();
    await flushPromises();

    clickBodyButton('新しい種別');
    await flushPromises();
    expect(document.body.textContent).toContain('種別を作成');

    await input('task-type-name').setValue('調査');
    await formEl().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID } },
      body: { name: '調査', color: '#ef4444' },
    });
    expect(document.body.textContent).not.toContain('種別を作成');
  });

  it('編集ダイアログは既存値をプリフィルし、保存で PUT を送る', async () => {
    updateMutateAsync.mockResolvedValue({ ...bugType, name: '不具合' });
    queryState.taskTypes = [bugType];
    mountView();
    await flushPromises();

    clickByAriaLabel('種別「バグ」を編集');
    await flushPromises();

    expect((input('task-type-name').element as HTMLInputElement).value).toBe('バグ');
    expect((input('task-type-color').element as HTMLInputElement).value).toBe('#ef4444');

    await input('task-type-name').setValue('不具合');
    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: bugType.id },
      },
      body: { name: '不具合', color: '#ef4444' },
    });
  });

  it('名前が空・色が不正なら送信せずエラーを表示する', async () => {
    mountView();
    await flushPromises();

    clickBodyButton('新しい種別');
    await flushPromises();

    await input('task-type-color').setValue('red');
    await formEl().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('名前は 1〜100 文字で入力してください');
    expect(document.body.textContent).toContain('色は #RRGGBB 形式で入力してください');
  });

  it('名前 100 文字は送信でき、101 文字は送信せずエラーを表示する', async () => {
    createMutateAsync.mockResolvedValue({ ...bugType, name: 'a'.repeat(100) });
    mountView();
    await flushPromises();

    clickBodyButton('新しい種別');
    await flushPromises();

    await input('task-type-name').setValue('a'.repeat(101));
    await formEl().trigger('submit');
    await flushPromises();
    expect(createMutateAsync).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('名前は 1〜100 文字で入力してください');

    // 100 文字ちょうどは成功する（絵文字 = UTF-16 では 200 単位でも通す）
    await input('task-type-name').setValue('😀'.repeat(100));
    await formEl().trigger('submit');
    await flushPromises();
    expect(createMutateAsync).toHaveBeenCalledTimes(1);
    expect(createMutateAsync.mock.calls[0][0].body.name).toBe('😀'.repeat(100));
  });

  it('保存リクエスト進行中はフォームダイアログのクローズ要求を無視する', async () => {
    mutationPending.post.value = true;
    mountView();
    await flushPromises();

    clickBodyButton('新しい種別');
    await flushPromises();
    expect(openDialog()?.textContent).toContain('種別を作成');

    pressEscape();
    await flushPromises();

    expect(openDialog()?.textContent).toContain('種別を作成');
  });

  it('削除は確認ダイアログで付いていたタスクの扱いを示し、DELETE を送る', async () => {
    deleteMutateAsync.mockResolvedValue(undefined);
    queryState.taskTypes = [bugType];
    mountView();
    await flushPromises();

    clickByAriaLabel('種別「バグ」を削除');
    await flushPromises();

    expect(document.body.textContent).toContain(
      '「バグ」を削除します。付いていたタスクは残り、種別だけが外れます。',
    );

    clickBodyButton('削除する');
    await flushPromises();

    expect(deleteMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: bugType.id },
      },
    });
    expect(openDialog()).toBeNull();
  });

  it('削除リクエスト進行中は確認ダイアログのクローズ要求を無視する', async () => {
    mutationPending.delete.value = true;
    queryState.taskTypes = [bugType];
    mountView();
    await flushPromises();

    clickByAriaLabel('種別「バグ」を削除');
    await flushPromises();
    pressEscape();
    await flushPromises();

    expect(openDialog()?.textContent).toContain('種別を削除しますか？');
  });

  it('保存失敗時はダイアログ内に、削除失敗時は確認ダイアログ内にエラーを表示する', async () => {
    createMutateAsync.mockRejectedValue(new Error('conflict'));
    deleteMutateAsync.mockRejectedValue(new Error('forbidden'));
    queryState.taskTypes = [bugType];
    mountView();
    await flushPromises();

    clickBodyButton('新しい種別');
    await flushPromises();
    await input('task-type-name').setValue('重複');
    await formEl().trigger('submit');
    await flushPromises();
    expect(document.body.textContent).toContain('種別を保存できませんでした');
    clickBodyButton('キャンセル');
    await flushPromises();

    clickByAriaLabel('種別「バグ」を削除');
    await flushPromises();
    clickBodyButton('削除する');
    await flushPromises();
    expect(document.body.textContent).toContain('種別を削除できませんでした');
    expect(document.body.textContent).toContain('種別を削除しますか？');
  });

  it('上へ・下へで並べ替えた全 ID を PUT /reorder に送る。端の行は動かせない', async () => {
    reorderMutateAsync.mockResolvedValue([]);
    queryState.taskTypes = [bugType, featureType];
    mountView();
    await flushPromises();

    expect(
      document.body
        .querySelector<HTMLButtonElement>('button[aria-label="種別「バグ」を上へ"]')
        ?.hasAttribute('disabled'),
    ).toBe(true);
    expect(
      document.body
        .querySelector<HTMLButtonElement>('button[aria-label="種別「機能」を下へ"]')
        ?.hasAttribute('disabled'),
    ).toBe(true);

    clickByAriaLabel('種別「機能」を上へ');
    await flushPromises();

    expect(reorderMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID } },
      body: { ids: [featureType.id, bugType.id] },
    });
  });

  it('並べ替えの応答を一覧のキャッシュへ先に入れる（再取得を待たずに次の操作が新しい並びから始まる）', async () => {
    const reordered = [
      { ...featureType, position: 0 },
      { ...bugType, position: 1 },
    ];
    reorderMutateAsync.mockResolvedValue(reordered);
    queryState.taskTypes = [bugType, featureType];
    mountView();
    await flushPromises();

    clickByAriaLabel('種別「機能」を上へ');
    await flushPromises();

    const cached = queryClient
      .getQueryCache()
      .findAll({ queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/task-types'] })
      .map((query) => query.state.data);
    expect(cached).toContainEqual(reordered);
  });

  it('一覧の再取得中は並べ替えのボタンを止める', async () => {
    queryState.taskTypes = [bugType, featureType];
    queryState.isFetching = true;
    mountView();
    await flushPromises();

    for (const name of ['種別「バグ」を下へ', '種別「機能」を上へ']) {
      expect(
        document.body.querySelector<HTMLButtonElement>(`button[aria-label="${name}"]`)?.disabled,
        name,
      ).toBe(true);
    }
  });

  it('名前や色を変えたら、詳細・一覧・検索・カレンダーのタスクのキャッシュも取り直す', async () => {
    updateMutateAsync.mockResolvedValue(bugType);
    queryState.taskTypes = [bugType];
    mountView();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    await flushPromises();

    clickByAriaLabel('種別「バグ」を編集');
    await flushPromises();
    await input('task-type-name').setValue('不具合');
    await formEl().trigger('submit');
    await flushPromises();

    const paths = invalidate.mock.calls.map(
      ([filter]) => (filter as { queryKey: unknown[] }).queryKey[1],
    );
    expect(paths).toEqual(
      expect.arrayContaining([
        '/v1/tenants/{tenant_id}/projects/{project_id}/tasks',
        '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}',
        '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search',
        '/v1/tenants/{tenant_id}/schedule',
      ]),
    );
  });

  it('削除したときも、タスクのキャッシュを取り直す', async () => {
    deleteMutateAsync.mockResolvedValue(undefined);
    queryState.taskTypes = [bugType];
    mountView();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    await flushPromises();

    clickByAriaLabel('種別「バグ」を削除');
    await flushPromises();
    clickBodyButton('削除する');
    await flushPromises();

    const paths = invalidate.mock.calls.map(
      ([filter]) => (filter as { queryKey: unknown[] }).queryKey[1],
    );
    expect(paths).toContain('/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}');
  });

  it('並べ替えに失敗したら理由を出す', async () => {
    reorderMutateAsync.mockRejectedValue(new Error('bad request'));
    queryState.taskTypes = [bugType, featureType];
    mountView();
    await flushPromises();

    clickByAriaLabel('種別「バグ」を下へ');
    await flushPromises();

    expect(document.body.textContent).toContain('並び順を変更できませんでした');
  });

  /** projectId が変わったらクエリキーも切り替わる（LabelsSection と同じ理由） */
  it('projectId が変わったらクエリキーが切り替わる', async () => {
    const OTHER_PROJECT_UUID = '00000000-0000-4000-8000-000000000020';
    queryState.taskTypes = [bugType];
    const wrapper = mountView();
    await flushPromises();

    const optionsRef = vi.mocked(useQuery).mock.calls.at(-1)?.[0] as {
      value: { queryKey: readonly unknown[] };
    };
    expect(JSON.stringify(optionsRef.value.queryKey)).toContain(PROJECT_UUID);

    await wrapper.setProps({ projectId: OTHER_PROJECT_UUID });
    await flushPromises();

    expect(JSON.stringify(optionsRef.value.queryKey)).toContain(OTHER_PROJECT_UUID);
  });
});

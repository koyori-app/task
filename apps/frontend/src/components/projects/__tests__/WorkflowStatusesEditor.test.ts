import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { createMutateAsync, updateMutateAsync, deleteMutateAsync, reorderMutateAsync, queryFn } =
  vi.hoisted(() => ({
    createMutateAsync: vi.fn(),
    updateMutateAsync: vi.fn(),
    deleteMutateAsync: vi.fn(),
    reorderMutateAsync: vi.fn(),
    queryFn: vi.fn(),
  }));

vi.mock('@/lib/api-vue-query', () => ({
  apiClient: {
    queryOptions: (_method: string, path: string) => ({
      queryKey: ['get', path],
      queryFn,
    }),
    useMutation: (method: string, path: string) => {
      const mutateAsync =
        method === 'post'
          ? createMutateAsync
          : method === 'delete'
            ? deleteMutateAsync
            : path.endsWith('/reorder')
              ? reorderMutateAsync
              : updateMutateAsync;
      return { mutateAsync, isPending: { value: false } };
    },
  },
}));

import WorkflowStatusesEditor from '../WorkflowStatusesEditor.vue';
import type { components } from '@/generated/api';

type ProjectStatus = components['schemas']['ProjectStatusResponse'];

const TENANT_ID = '11111111-1111-4111-8111-111111111111';
const PROJECT_ID = '22222222-2222-4222-8222-222222222222';
const TODO_ID = '33333333-3333-4333-8333-333333333333';
const PROGRESS_ID = '44444444-4444-4444-8444-444444444444';
const DONE_ID = '55555555-5555-4555-8555-555555555555';
const STATUSES_PATH = `/v1/tenants/{tenant_id}/projects/{project_id}/statuses`;
const STATUS_PATH = `${STATUSES_PATH}/{id}`;

const baseStatuses: ProjectStatus[] = [
  {
    id: TODO_ID,
    project_id: PROJECT_ID,
    name: 'Todo',
    color: '#64748b',
    position: 0,
    is_default: true,
    is_done_state: false,
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: PROGRESS_ID,
    project_id: PROJECT_ID,
    name: 'In Progress',
    color: '#2563eb',
    position: 1,
    is_default: false,
    is_done_state: false,
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: DONE_ID,
    project_id: PROJECT_ID,
    name: 'Done',
    color: '#16a34a',
    position: 2,
    is_default: false,
    is_done_state: true,
    created_at: '2026-01-01T00:00:00Z',
  },
];

let statuses: ProjectStatus[];
let wrapper: VueWrapper;
let queryClient: QueryClient;

function checkbox(statusName: string, label: 'Default' | 'Done state') {
  const row = wrapper
    .findAll('li')
    .find((item) => item.find(`input[aria-label="${statusName}の名前"]`).exists());
  if (!row) throw new Error(`status row not found: ${statusName}`);
  const target = row.findAll('label').find((item) => item.text().trim() === label);
  if (!target) throw new Error(`checkbox not found: ${label}`);
  return target.find('button');
}

async function mountEditor(nextStatuses: ProjectStatus[] = baseStatuses) {
  statuses = structuredClone(nextStatuses);
  queryFn.mockImplementation(() => structuredClone(statuses));
  queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  wrapper = mount(WorkflowStatusesEditor, {
    props: { tenantId: TENANT_ID, projectId: PROJECT_ID },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
  await flushPromises();
}

describe('WorkflowStatusesEditor', () => {
  beforeEach(() => {
    createMutateAsync.mockReset().mockResolvedValue(undefined);
    updateMutateAsync.mockReset().mockResolvedValue(undefined);
    deleteMutateAsync.mockReset().mockResolvedValue(undefined);
    reorderMutateAsync.mockReset().mockResolvedValue(undefined);
    queryFn.mockReset();
  });

  afterEach(() => {
    wrapper?.unmount();
    document.body.innerHTML = '';
    vi.restoreAllMocks();
  });

  it('CRUD で UUID path/body と削除時の migrate_to_status_id を送る', async () => {
    await mountEditor();

    await wrapper.get('input[aria-label="新しいステータス名"]').setValue('Review');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID } },
      body: {
        name: 'Review',
        color: '#64748b',
        position: 3,
        is_default: false,
        is_done_state: false,
      },
    });

    await wrapper.get('input[aria-label="In Progressの名前"]').setValue('Development');
    await wrapper.get('button[aria-label="In Progressを保存"]').trigger('click');
    await flushPromises();
    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id: PROGRESS_ID },
      },
      body: { name: 'Development', color: '#2563eb' },
    });

    await wrapper.get('button[aria-label="In Progressを削除"]').trigger('click');
    await flushPromises();
    const confirmDelete = [...document.body.querySelectorAll('button')].find(
      (item) => item.textContent?.trim() === '削除する',
    );
    if (!confirmDelete) throw new Error('confirm delete button not found');
    confirmDelete.click();
    await flushPromises();
    expect(deleteMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id: PROGRESS_ID },
        query: { migrate_to_status_id: TODO_ID },
      },
    });
  });

  it('Default と Done state の切替はいずれも新対象への単発 PUT になる', async () => {
    await mountEditor();

    await checkbox('In Progress', 'Default').trigger('click');
    await flushPromises();
    expect(updateMutateAsync).toHaveBeenLastCalledWith({
      params: {
        path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id: PROGRESS_ID },
      },
      body: { is_default: true },
    });

    updateMutateAsync.mockClear();
    await checkbox('In Progress', 'Done state').trigger('click');
    await flushPromises();
    expect(updateMutateAsync).toHaveBeenCalledTimes(1);
    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: {
        path: { tenant_id: TENANT_ID, project_id: PROJECT_ID, id: PROGRESS_ID },
      },
      body: { is_done_state: true },
    });
  });

  it('Default/Done mutation の失敗時にも一覧を再取得して DB と同期する', async () => {
    await mountEditor();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    updateMutateAsync.mockRejectedValue(new Error('conflict'));

    await checkbox('In Progress', 'Default').trigger('click');
    await flushPromises();
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['get', STATUSES_PATH] });

    invalidate.mockClear();
    await checkbox('In Progress', 'Done state').trigger('click');
    await flushPromises();
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['get', STATUSES_PATH] });
    expect(wrapper.text()).toContain('ステータスの種別を変更できませんでした');
  });

  it.each([
    ['最後の1件', [baseStatuses[0]!], '最後のステータスは削除できません'],
    ['Default', baseStatuses, 'Default のステータスは削除できません'],
    [
      '唯一の Done state',
      [
        { ...baseStatuses[0]!, is_default: true, is_done_state: false },
        { ...baseStatuses[2]!, is_default: false, is_done_state: true },
      ],
      '唯一の Done state は削除できません',
    ],
  ])('%s の削除を拒否する', async (_case, data, message) => {
    await mountEditor(data);
    const target = data.length === 1 ? data[0]! : data.at(-1)!;
    if (_case === 'Default') {
      await wrapper.get('button[aria-label="Todoを削除"]').trigger('click');
    } else {
      await wrapper.get(`button[aria-label="${target.name}を削除"]`).trigger('click');
    }
    await flushPromises();
    expect(wrapper.text()).toContain(message);
    expect(deleteMutateAsync).not.toHaveBeenCalled();
  });

  it('reorder 失敗時にエラーを表示する', async () => {
    reorderMutateAsync.mockRejectedValue(new Error('conflict'));
    await mountEditor();

    await wrapper.get('button[aria-label="Todoを下へ"]').trigger('click');
    await flushPromises();

    expect(reorderMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_ID, project_id: PROJECT_ID } },
      body: { ids: [PROGRESS_ID, TODO_ID, DONE_ID] },
    });
    expect(wrapper.text()).toContain('並び順を変更できませんでした');
  });

  it('削除進行中は native cancel/Esc 相当を拒否してダイアログを維持する', async () => {
    let resolveDelete: (() => void) | undefined;
    deleteMutateAsync.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveDelete = resolve;
        }),
    );
    await mountEditor();
    await wrapper.get('button[aria-label="In Progressを削除"]').trigger('click');
    await flushPromises();

    const confirmDelete = [...document.body.querySelectorAll('button')].find(
      (item) => item.textContent?.trim() === '削除する',
    );
    if (!confirmDelete) throw new Error('confirm delete button not found');
    confirmDelete.click();
    await flushPromises();

    const dialog = document.body.querySelector<HTMLElement>('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(document.body.querySelector('[data-slot="dialog-close"]')).toBeNull();
    const cancel = new Event('cancel', { bubbles: true, cancelable: true });
    dialog!.dispatchEvent(cancel);
    await flushPromises();

    expect(cancel.defaultPrevented).toBe(true);
    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull();

    resolveDelete?.();
    await flushPromises();
  });

  it.each([
    ['ASCII 100', 'a'.repeat(100), true],
    ['ASCII 101', 'a'.repeat(101), false],
    ['日本語 100', '界'.repeat(100), true],
    ['日本語 101', '界'.repeat(101), false],
    ['絵文字 100', '😀'.repeat(100), true],
    ['絵文字 101', '😀'.repeat(101), false],
  ])('create/edit の %s code point 境界を検証する', async (_case, name, valid) => {
    await mountEditor();

    await wrapper.get('input[aria-label="新しいステータス名"]').setValue(name);
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(createMutateAsync).toHaveBeenCalledTimes(valid ? 1 : 0);

    await wrapper.get('input[aria-label="In Progressの名前"]').setValue(name);
    await wrapper.get('button[aria-label="In Progressを保存"]').trigger('click');
    await flushPromises();
    expect(updateMutateAsync).toHaveBeenCalledTimes(valid ? 1 : 0);
    if (!valid) expect(wrapper.text()).toContain('ステータス名は1〜100文字で入力してください');
  });

  it('request path constants remain typed UUID template paths', () => {
    expect(STATUS_PATH).toBe('/v1/tenants/{tenant_id}/projects/{project_id}/statuses/{id}');
  });
});

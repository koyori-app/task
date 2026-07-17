import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, DOMWrapper, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

const { createMutateAsync, navigateMock } = vi.hoisted(() => ({
  createMutateAsync: vi.fn(),
  navigateMock: vi.fn(),
}));

vi.mock('vike/client/router', () => ({
  navigate: navigateMock,
}));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn(() => ({
        mutateAsync: createMutateAsync,
        isPending: { value: false },
      })),
    },
  };
});

import ProjectCreateForm from '../ProjectCreateForm.vue';

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const createdProject = {
  id: 'proj-1',
  tenant_id: TENANT_UUID,
  name: 'New Proj',
  description: '',
  key: 'NEWPROJ',
  is_personal: false,
  icon_emoji: null,
  icon_url: null,
  personal_owner_id: null,
};

function mountForm() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ProjectCreateForm, {
    props: { tenantId: TENANT_UUID, tenantSlug: 'acme' },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function input(id: string) {
  const el = document.body.querySelector<HTMLInputElement>(`#${id}`);
  if (!el) throw new Error(`input #${id} not found`);
  return new DOMWrapper(el);
}

function formEl() {
  const el = document.body.querySelector('form');
  if (!el) throw new Error('form not found');
  return new DOMWrapper(el);
}

describe('ProjectCreateForm', () => {
  beforeEach(() => {
    createMutateAsync.mockReset();
    navigateMock.mockReset();
    document.body.innerHTML = '';
  });

  it('名前からキーを自動提案し、POST 後に新プロジェクトへ遷移する', async () => {
    createMutateAsync.mockResolvedValue(createdProject);
    mountForm();
    await flushPromises();

    await input('name').setValue('New Proj');
    expect((input('key').element as HTMLInputElement).value).toBe('NEWPROJ');

    await formEl().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID } },
      body: { name: 'New Proj', key: 'NEWPROJ' },
    });
    expect(navigateMock).toHaveBeenCalledWith('/acme/projects/NEWPROJ/tasks');
  });

  it('キー空欄なら key を送らない（backend 自動生成に委ねる）', async () => {
    createMutateAsync.mockResolvedValue(createdProject);
    mountForm();
    await flushPromises();

    await input('name').setValue('プロジェクト');
    await formEl().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID } },
      body: { name: 'プロジェクト' },
    });
  });

  it('既定ステータスのプレビュー（seed セットと同一）を表示する', async () => {
    mountForm();
    await flushPromises();

    const text = document.body.textContent ?? '';
    for (const name of ['Backlog', 'Todo', 'In Progress', 'Done']) {
      expect(text).toContain(name);
    }
    expect(text).toContain('既定のセットで作成され、後から編集できます');
    expect(text).toContain('Default');
    expect(text).toContain('Done state');
  });

  it('失敗時はエラーを表示し、遷移しない', async () => {
    createMutateAsync.mockRejectedValue(new Error('server error'));
    mountForm();
    await flushPromises();

    await input('name').setValue('X Project');
    await formEl().trigger('submit');
    await flushPromises();

    expect(document.body.textContent).toContain('プロジェクトを作成できませんでした');
    expect(navigateMock).not.toHaveBeenCalled();
  });
});

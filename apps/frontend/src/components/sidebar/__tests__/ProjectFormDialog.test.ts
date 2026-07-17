import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises, DOMWrapper, enableAutoUnmount } from '@vue/test-utils';
import { afterEach } from 'vitest';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

const { createMutateAsync, updateMutateAsync } = vi.hoisted(() => ({
  createMutateAsync: vi.fn(),
  updateMutateAsync: vi.fn(),
}));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn((method: string) => ({
        mutateAsync: method === 'post' ? createMutateAsync : updateMutateAsync,
        isPending: { value: false },
      })),
    },
  };
});

import ProjectFormDialog from '../ProjectFormDialog.vue';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const sampleProject: ProjectResponse = {
  id: '00000000-0000-4000-8000-000000000010',
  tenant_id: TENANT_UUID,
  name: 'Team Alpha',
  description: 'Shared project',
  key: 'ALPHA',
  is_personal: false,
  icon_emoji: null,
  icon_url: null,
  personal_owner_id: null,
};

function mountDialog(props: { project?: ProjectResponse | null } = {}) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ProjectFormDialog, {
    props: { open: true, tenantId: TENANT_UUID, ...props },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function bodyInput(id: string) {
  const el = document.body.querySelector<HTMLInputElement>(`#${id}`);
  if (!el) throw new Error(`input #${id} not found`);
  return new DOMWrapper(el);
}

function bodyForm() {
  const el = document.body.querySelector('form');
  if (!el) throw new Error('form not found');
  return new DOMWrapper(el);
}

describe('ProjectFormDialog', () => {
  beforeEach(() => {
    createMutateAsync.mockReset();
    updateMutateAsync.mockReset();
    document.body.innerHTML = '';
  });

  it('作成モード: 名前からキーを自動提案し、POST に name/key を送る', async () => {
    createMutateAsync.mockResolvedValue({ ...sampleProject, key: 'NEWPROJ' });
    const wrapper = mountDialog();
    await flushPromises();

    await bodyInput('name').setValue('New Proj');
    expect((bodyInput('key').element as HTMLInputElement).value).toBe('NEWPROJ');

    await bodyForm().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID } },
      body: { name: 'New Proj', key: 'NEWPROJ' },
    });
    expect(wrapper.emitted('saved')?.[0]).toEqual([{ ...sampleProject, key: 'NEWPROJ' }]);
    expect(wrapper.emitted('update:open')?.at(-1)).toEqual([false]);
  });

  it('作成モード: キー空欄なら key を送らない（backend 自動生成に委ねる）', async () => {
    createMutateAsync.mockResolvedValue(sampleProject);
    mountDialog();
    await flushPromises();

    await bodyInput('name').setValue('プロジェクト');
    // 日本語名 → suggestKey は空 → key は送られない
    await bodyForm().trigger('submit');
    await flushPromises();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID } },
      body: { name: 'プロジェクト' },
    });
  });

  it('編集モード: 既存値をプリフィルし、PUT に name/description を送る', async () => {
    updateMutateAsync.mockResolvedValue({ ...sampleProject, name: 'Renamed' });
    const wrapper = mountDialog({ project: sampleProject });
    // open は初期 true のため watch を発火させる
    await wrapper.setProps({ open: false });
    await wrapper.setProps({ open: true });
    await flushPromises();

    expect((bodyInput('name').element as HTMLInputElement).value).toBe('Team Alpha');

    await bodyInput('name').setValue('Renamed');
    await bodyForm().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: sampleProject.id } },
      body: { name: 'Renamed', description: 'Shared project' },
    });
    expect(createMutateAsync).not.toHaveBeenCalled();
  });

  it('編集モード: キー入力欄は無効化表示（編集不可）', async () => {
    const wrapper = mountDialog({ project: sampleProject });
    await wrapper.setProps({ open: false });
    await wrapper.setProps({ open: true });
    await flushPromises();

    const keyInput = document.body.querySelector<HTMLInputElement>('#project-key-readonly');
    expect(keyInput).not.toBeNull();
    expect(keyInput!.disabled).toBe(true);
    expect(keyInput!.value).toBe('ALPHA');
  });

  it('失敗時はエラーを表示し、ダイアログを閉じない', async () => {
    createMutateAsync.mockRejectedValue(new Error('server error'));
    const wrapper = mountDialog();
    await flushPromises();

    await bodyInput('name').setValue('X Project');
    await bodyForm().trigger('submit');
    await flushPromises();

    expect(document.body.textContent).toContain('プロジェクトを作成できませんでした');
    expect(wrapper.emitted('update:open')).toBeUndefined();
    expect(wrapper.emitted('saved')).toBeUndefined();
  });
});

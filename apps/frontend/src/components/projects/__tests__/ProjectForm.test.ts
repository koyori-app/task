import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, DOMWrapper, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

const { createMutateAsync, updateMutateAsync, navigateMock } = vi.hoisted(() => ({
  createMutateAsync: vi.fn(),
  updateMutateAsync: vi.fn(),
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
      useMutation: vi.fn((method: string) => ({
        mutateAsync: method === 'post' ? createMutateAsync : updateMutateAsync,
        isPending: { value: false },
      })),
    },
  };
});

import ProjectForm from '../ProjectForm.vue';
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
  icon_emoji: '🎨',
  icon_url: null,
  personal_owner_id: null,
};

function mountForm(props: { project?: ProjectResponse | null } = {}) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ProjectForm, {
    props: { tenantId: TENANT_UUID, tenantSlug: 'acme', ...props },
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

describe('ProjectForm', () => {
  beforeEach(() => {
    createMutateAsync.mockReset();
    updateMutateAsync.mockReset();
    navigateMock.mockReset();
    document.body.innerHTML = '';
  });

  it('作成モード: 名前からキーを自動提案し、POST 後に新プロジェクトへ遷移する', async () => {
    createMutateAsync.mockResolvedValue({ ...sampleProject, key: 'NEWPROJ' });
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

  it('作成モード: キー空欄なら key を送らない（backend 自動生成に委ねる）', async () => {
    createMutateAsync.mockResolvedValue(sampleProject);
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

  it('設定モード: 既存値をプリフィルし、PUT に name/description を送る', async () => {
    updateMutateAsync.mockResolvedValue({ ...sampleProject, name: 'Renamed' });
    mountForm({ project: sampleProject });
    await flushPromises();

    expect((input('name').element as HTMLInputElement).value).toBe('Team Alpha');

    await input('name').setValue('Renamed');
    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: sampleProject.id } },
      body: { name: 'Renamed', description: 'Shared project' },
    });
    expect(createMutateAsync).not.toHaveBeenCalled();
  });

  it('設定モード: アイコンを外すと clear_icon_emoji を送る', async () => {
    updateMutateAsync.mockResolvedValue({ ...sampleProject, icon_emoji: null });
    const wrapper = mountForm({ project: sampleProject });
    await flushPromises();

    // 絵文字メニューを開いて「アイコンなし」を選択
    await wrapper.find('button[aria-label="アイコンを選択"]').trigger('click');
    const clearButton = [...document.body.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'アイコンなし',
    );
    expect(clearButton).toBeTruthy();
    clearButton!.click();
    await flushPromises();

    await formEl().trigger('submit');
    await flushPromises();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: sampleProject.id } },
      body: { name: 'Team Alpha', description: 'Shared project', clear_icon_emoji: true },
    });
  });

  it('設定モード: キー入力欄は無効化表示（編集不可）で Danger zone が出る', async () => {
    mountForm({ project: sampleProject });
    await flushPromises();

    const keyInput = document.body.querySelector<HTMLInputElement>('#project-key-readonly');
    expect(keyInput).not.toBeNull();
    expect(keyInput!.disabled).toBe(true);
    expect(keyInput!.value).toBe('ALPHA');
    expect(document.body.textContent).toContain('Danger zone');
  });

  it('作成モード: Danger zone は出ない', async () => {
    mountForm();
    await flushPromises();
    expect(document.body.textContent).not.toContain('Danger zone');
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

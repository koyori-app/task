import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

// vi.hoisted は import 初期化前に実行されるため vue の ref は使えない。
// コンポーネントは .value を読むだけなので素のオブジェクトで足りる
const { deleteMutateAsync, deletePending } = vi.hoisted(() => ({
  deleteMutateAsync: vi.fn(),
  deletePending: { value: false },
}));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn(() => ({
        mutateAsync: deleteMutateAsync,
        isPending: deletePending,
      })),
    },
  };
});

import DeleteProjectDialog from '../DeleteProjectDialog.vue';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const sampleProject: ProjectResponse = {
  id: '00000000-0000-4000-8000-000000000010',
  tenant_id: TENANT_UUID,
  name: 'Team Alpha',
  description: '',
  key: 'ALPHA',
  is_personal: false,
  icon_emoji: null,
  icon_url: null,
  personal_owner_id: null,
};

function mountDialog() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(DeleteProjectDialog, {
    props: { open: true, tenantId: TENANT_UUID, project: sampleProject },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function clickBodyButton(label: string) {
  const button = [...document.body.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!button) throw new Error(`button "${label}" not found`);
  button.click();
}

describe('DeleteProjectDialog', () => {
  beforeEach(() => {
    deleteMutateAsync.mockReset();
    deletePending.value = false;
    document.body.innerHTML = '';
  });

  it('確認で DELETE を送り、deleted を emit して閉じる', async () => {
    deleteMutateAsync.mockResolvedValue(undefined);
    const wrapper = mountDialog();
    await flushPromises();

    expect(document.body.textContent).toContain('「Team Alpha」を削除します');
    clickBodyButton('削除する');
    await flushPromises();

    expect(deleteMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, id: sampleProject.id } },
    });
    expect(wrapper.emitted('deleted')?.[0]).toEqual([sampleProject]);
    expect(wrapper.emitted('update:open')?.at(-1)).toEqual([false]);
  });

  it('失敗時はエラーを表示し、閉じずに deleted も emit しない', async () => {
    deleteMutateAsync.mockRejectedValue(new Error('forbidden'));
    const wrapper = mountDialog();
    await flushPromises();

    clickBodyButton('削除する');
    await flushPromises();

    expect(document.body.textContent).toContain('プロジェクトを削除できませんでした');
    expect(wrapper.emitted('deleted')).toBeUndefined();
    expect(wrapper.emitted('update:open')).toBeUndefined();
  });

  it('削除進行中はダイアログのクローズ要求を無視する', async () => {
    deletePending.value = true;
    const wrapper = mountDialog();
    await flushPromises();

    // reka-ui の update:open(false)（Esc 等）を直接シミュレート
    const dialog = wrapper.findComponent({ name: 'DialogRoot' });
    dialog.vm.$emit('update:open', false);
    await flushPromises();

    expect(wrapper.emitted('update:open')).toBeUndefined();
  });
});

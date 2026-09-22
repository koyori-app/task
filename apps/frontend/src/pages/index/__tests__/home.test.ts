import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { useTenantStore } from '@/stores/tenant';
const { navigate } = vi.hoisted(() => ({ navigate: vi.fn().mockResolvedValue(undefined) }));
vi.mock('vike/client/router', () => ({ navigate }));
import Page from '../+Page.vue';

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  navigate.mockClear();
});
function setup(data: unknown, selected: string | null = null, status = 200) {
  const pinia = createPinia();
  setActivePinia(pinia);
  useTenantStore().selectedTenantId = selected;
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(JSON.stringify(data), {
          status,
          headers: { 'Content-Type': 'application/json' },
        }),
    ),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(Page, {
    global: {
      plugins: [pinia, [VueQueryPlugin, { queryClient: client }]],
      stubs: { CreateTenantDialog: true },
    },
  });
}
describe('トップページ', () => {
  it.each([
    ['second', '/two'],
    ['removed', '/one'],
    [null, '/one'],
  ])('保存済みの選択 %s からホーム %s に移る', async (selected, expected) => {
    setup(
      [
        { id: 'first', display_id: 'one' },
        { id: 'second', display_id: 'two' },
      ],
      selected,
    );
    await flushPromises();
    expect(navigate).toHaveBeenCalledWith(expected, { overwriteLastHistoryEntry: true });
  });
  it('所属なしは作成導線、取得失敗は再試行を表示する', async () => {
    const empty = setup([]);
    await flushPromises();
    expect(empty.text()).toContain('ワークスペースを作成');
    expect(navigate).not.toHaveBeenCalled();
    empty.unmount();
    const error = setup({}, null, 503);
    await flushPromises();
    expect(error.get('[role="alert"]').text()).toContain('ホームを開けませんでした');
    expect(error.text()).toContain('再試行');
    expect(navigate).not.toHaveBeenCalled();
  });
});

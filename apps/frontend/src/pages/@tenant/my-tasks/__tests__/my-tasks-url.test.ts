import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reactive, ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

const { navigate, GET, pageContext } = vi.hoisted(() => ({
  navigate: vi.fn(),
  GET: vi.fn(),
  pageContext: {
    value: null as null | {
      routeParams: Record<string, string>;
      urlParsed: { search: Record<string, string> };
    },
  },
}));
vi.mock('vike/client/router', () => ({ navigate }));
vi.mock('vike-vue/usePageContext', () => ({ usePageContext: () => pageContext.value }));
vi.mock('@/composables/useResolvedTenantId', () => ({
  useResolvedTenantId: () => ({
    tenantId: ref('tenant-id'),
    isTenantNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
    tenantsQuery: { refetch: vi.fn() },
  }),
}));
vi.mock('@/lib/api-vue-query', () => ({ fetchClient: { GET } }));

import Page from '../+Page.vue';

enableAutoUnmount(afterEach);
beforeEach(() => {
  pageContext.value = reactive({ routeParams: { tenant: 'acme' }, urlParsed: { search: {} } });
});
afterEach(() => {
  GET.mockReset();
  navigate.mockClear();
  vi.restoreAllMocks();
  window.history.replaceState(null, '', '/');
});

function task(n: number) {
  return {
    id: `t-${n}`,
    seq_id: n,
    title: `タスク${n}`,
    project: { id: 'p-1', key: 'ENG', name: 'Engineering', is_personal: false },
    status: { id: 's-1', name: 'Todo', color: '#888', is_done_state: false },
    priority: 'Medium',
    soft_deadline: null,
    hard_deadline: null,
  };
}

/** `total` 件あるものとして、要求の limit / offset どおりに返す */
function respondWith(total: number) {
  GET.mockImplementation(
    async (_path: string, { params }: { params: { query: Record<string, unknown> } }) => {
      const limit = Number(params.query.limit);
      const offset = Number(params.query.offset);
      const tasks = Array.from({ length: Math.max(0, Math.min(limit, total - offset)) }, (_, i) =>
        task(offset + i + 1),
      );
      return { data: { tasks, total } };
    },
  );
}

function mountPage(search: Record<string, string> = {}) {
  pageContext.value!.urlParsed.search = search;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(Page, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
}

const queries = () =>
  GET.mock.calls.map(([, { params }]) => params.query as Record<string, unknown>);
const tab = (wrapper: ReturnType<typeof mountPage>, label: string) =>
  wrapper.findAll('button').find((b) => b.text() === label)!;
const urlParams = () => new URL(window.location.href).searchParams;

describe('My Tasks のタブと件数を URL に持つ（TASK-474）', () => {
  it('?tab=week で開くと今週のタブで取得する', async () => {
    respondWith(1);
    const wrapper = mountPage({ tab: 'week' });
    await flushPromises();
    expect(queries()[0]).toMatchObject({ filter: 'week', limit: 50, offset: 0 });
    expect(tab(wrapper, '今週').attributes('data-variant')).toBe('primary');
    expect(tab(wrapper, '今日').attributes('data-variant')).toBe('tertiary');
  });

  it('タブを押すと URL に tab が載り、今日へ戻すと消える。履歴は積まない', async () => {
    respondWith(1);
    const pushState = vi.spyOn(window.history, 'pushState');
    const wrapper = mountPage();
    await flushPromises();
    await tab(wrapper, '期限超過').trigger('click');
    await flushPromises();
    expect(urlParams().get('tab')).toBe('overdue');
    expect(queries().at(-1)).toMatchObject({ filter: 'overdue', offset: 0 });

    await tab(wrapper, '今日').trigger('click');
    await flushPromises();
    expect(urlParams().has('tab')).toBe(false);
    expect(pushState).not.toHaveBeenCalled();
  });

  it('?shown=100 で開くと 100 件を 1 回で取り、「もっと見る」は続きから取って shown を増やす', async () => {
    respondWith(300);
    const wrapper = mountPage({ shown: '100' });
    await flushPromises();
    expect(queries()[0]).toMatchObject({ limit: 100, offset: 0 });
    expect(wrapper.findAll('a').length).toBe(100);

    await tab(wrapper, 'もっと見る').trigger('click');
    await flushPromises();
    expect(queries().at(-1)).toMatchObject({ limit: 50, offset: 100 });
    expect(wrapper.findAll('a').length).toBe(150);
    expect(urlParams().get('shown')).toBe('150');
  });

  it('今日が 0 件なら今日用の文言を出し、「すべて」が 0 件なら割り当て 0 件と出す', async () => {
    respondWith(0);
    const wrapper = mountPage();
    await flushPromises();
    expect(wrapper.text()).toContain('今日が期限の未完了タスクはありません');
    expect(wrapper.text()).not.toContain('割り当てられたタスクは0件です');

    await tab(wrapper, 'すべて').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('割り当てられたタスクは0件です');
  });

  it('同じページへの遷移で URL の tab が消えると今日へ戻る', async () => {
    respondWith(1);
    const wrapper = mountPage({ tab: 'week' });
    await flushPromises();
    pageContext.value!.urlParsed = { search: {} };
    await flushPromises();
    expect(tab(wrapper, '今日').attributes('data-variant')).toBe('primary');
    expect(queries().at(-1)).toMatchObject({ filter: 'today', limit: 50, offset: 0 });
  });
});

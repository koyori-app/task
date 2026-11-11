import { afterEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

const { navigate, GET } = vi.hoisted(() => ({ navigate: vi.fn(), GET: vi.fn() }));
vi.mock('vike/client/router', () => ({ navigate }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({ routeParams: { tenant: 'acme' }, urlParsed: { searchAll: {} } }),
}));
vi.mock('@/composables/useResolvedTenantId', () => ({
  useResolvedTenantId: () => ({
    tenantId: ref('tenant-id'),
    isTenantNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
  }),
}));
vi.mock('@/lib/api-vue-query', () => ({ fetchClient: { GET } }));
// 広い画面（オーバーレイを出す幅）
vi.mock('@vueuse/core', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@vueuse/core')>()),
  useMediaQuery: () => ref(true),
}));
// 器の中身（TaskDetailPane）は自前で API を叩くので、props と emit だけの代役にする
vi.mock('@/components/tasks/TaskDetailOverlay.vue', async () => {
  const { defineComponent, h } = await import('vue');
  return {
    default: defineComponent({
      name: 'TaskDetailOverlay',
      props: ['open', 'tenantDisplayId', 'projectKey', 'taskId'],
      emits: ['open-task', 'open-task-ref', 'update:open'],
      setup: () => () => h('div'),
    }),
  };
});

import Page from '../+Page.vue';

enableAutoUnmount(afterEach);
afterEach(() => {
  navigate.mockClear();
  window.history.replaceState(null, '', '/');
});

function mountPage() {
  GET.mockResolvedValue({
    data: {
      // 今日の締切 = 表示中の 2 週間に必ず入る
      tasks: [
        {
          id: 't-1',
          seq_id: 1,
          seq_key: 'ENG-1',
          title: 'タスク',
          project: { id: 'p-1', key: 'ENG', name: 'Engineering', is_personal: false },
          status: { id: 's-1', name: 'Todo', color: '#888', is_done_state: false },
          priority: 'Medium',
          progress_pct: 0,
          assignees: [{ user_id: 'u-alice', role: 'primary' }],
          soft_deadline: new Date().toISOString(),
        },
      ],
      members: [{ id: 'u-alice', username: 'alice' }],
      sprints: [],
      milestones: [],
      range: { from: '', to: '' },
      next_cursor: null,
    },
  });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(Page, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
}

describe('カレンダーのタスク詳細（TASK-303）', () => {
  it('帯のクリックはページ遷移せずオーバーレイで開き、中の KEY-N・子タスクは積んで閉じると戻る', async () => {
    const wrapper = mountPage();
    await flushPromises();
    expect(wrapper.findComponent({ name: 'TaskDetailOverlay' }).exists()).toBe(false);

    await wrapper.get('a[data-task-id="t-1"]').trigger('click');
    const overlay = () => wrapper.getComponent({ name: 'TaskDetailOverlay' });
    expect(overlay().props()).toMatchObject({ open: true, projectKey: 'ENG', taskId: 'ENG-1' });
    expect(navigate).not.toHaveBeenCalled();
    // タスク一覧の ?selected= と同じく URL に持つ（replaceState）
    expect(new URL(window.location.href).searchParams.get('selected')).toBe('ENG-1');

    // カレンダーはテナント横断なので、別プロジェクトの KEY-N もオーバーレイで開く
    overlay().vm.$emit('open-task-ref', {
      projectKey: 'OPS',
      seqKey: 'OPS-3',
      href: '/acme/projects/OPS/tasks/OPS-3',
    });
    await flushPromises();
    expect(overlay().props()).toMatchObject({ open: true, projectKey: 'OPS', taskId: 'OPS-3' });

    // 子タスクは開いているタスクのプロジェクトで引く
    overlay().vm.$emit('open-task', { seq_id: 9 });
    await flushPromises();
    expect(overlay().props()).toMatchObject({ projectKey: 'OPS', taskId: 'OPS-9' });

    overlay().vm.$emit('update:open', false);
    await flushPromises();
    expect(overlay().props()).toMatchObject({ open: true, taskId: 'OPS-3' });

    overlay().vm.$emit('update:open', false);
    await flushPromises();
    expect(overlay().props()).toMatchObject({ open: true, projectKey: 'ENG', taskId: 'ENG-1' });

    overlay().vm.$emit('update:open', false);
    await flushPromises();
    expect(overlay().props('open')).toBe(false);
    expect(new URL(window.location.href).searchParams.has('selected')).toBe(false);
    expect(navigate).not.toHaveBeenCalled();
  });
});

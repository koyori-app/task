import { afterEach, expect, it, vi } from 'vitest';
import { defineComponent, h } from 'vue';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import TaskRefHoverCard from '../TaskRefHoverCard.vue';
import TaskRefCard from '../TaskRefCard.vue';

enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

// HoverCard の開閉通知だけを代行し、実際の composable の取得と表示を確かめる。
const HoverCard = defineComponent({
  props: ['onOpenChange'],
  setup(props, { slots }) {
    return () =>
      h('div', [
        h(
          'button',
          {
            onClick: () => {
              const link = document.createElement('a');
              link.href = '/acme/projects/TASK/tasks/TASK-58';
              props.onOpenChange(true, link);
            },
          },
          'open',
        ),
        slots.content?.(),
      ]);
  },
});
it.each([200, 500])(
  '開く前は通信せず、ステータス %i でも完了日時を締切に反映する',
  async (statusCode) => {
    const fetch = vi.fn(async (req: Request) => {
      const body = req.url.endsWith('/projects')
        ? [{ id: 'project', key: 'TASK', name: '開発' }]
        : req.url.endsWith('/statuses')
          ? statusCode === 200
            ? []
            : { message: 'failure' }
          : {
              title: '完了済み',
              assignees: [],
              soft_deadline: '2020-01-01T00:00:00Z',
              completed_at: '2019-12-31T00:00:00Z',
            };
      return new Response(JSON.stringify(body), {
        status: req.url.endsWith('/statuses') ? statusCode : 200,
        headers: { 'Content-Type': 'application/json' },
      });
    });
    vi.stubGlobal('fetch', fetch);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = mount(TaskRefHoverCard, {
      props: { container: null, tenantId: 'tenant', tenantDisplayId: 'acme' },
      global: { plugins: [[VueQueryPlugin, { queryClient: client }]], stubs: { HoverCard } },
    });
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(fetch).not.toHaveBeenCalled();
    await wrapper.get('button').trigger('click');
    await vi.waitFor(() => expect(wrapper.getComponent(TaskRefCard).props('state')).toBe('ready'));
    expect(wrapper.getComponent(TaskRefCard).props('deadline')?.overdue).toBe(false);
    expect(wrapper.text()).not.toContain('超過');
  },
);

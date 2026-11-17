import { afterEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';

const { GET, searchAll, media } = vi.hoisted(() => ({
  GET: vi.fn(),
  searchAll: { value: {} as Record<string, string[]> },
  // 既定は jsdom と同じ狭い画面。オーバーレイの試験だけ広い画面にする
  media: { wide: false },
}));
vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({
    routeParams: { tenant: 'acme' },
    urlParsed: { searchAll: searchAll.value },
  }),
}));
vi.mock('@vueuse/core', async (importOriginal) => {
  const { ref } = await import('vue');
  return {
    ...(await importOriginal<typeof import('@vueuse/core')>()),
    useMediaQuery: () => ref(media.wide),
  };
});
vi.mock('@/composables/useResolvedTenantId', () => ({
  useResolvedTenantId: () => ({
    tenantId: ref('tenant-id'),
    isTenantNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
  }),
}));
vi.mock('@/composables/useResolvedProjectId', () => ({
  useResolvedProjectId: () => ({
    projectId: ref('p-eng'),
    isProjectNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
  }),
}));
vi.mock('@/lib/api-vue-query', () => ({ fetchClient: { GET } }));
// 棒は wasm と canvas で描くので、渡された課題だけを見る代役にする
vi.mock('@koyori-app/arc-vue', async () => {
  const { defineComponent, h } = await import('vue');
  return {
    GanttChart: defineComponent({
      name: 'GanttChart',
      props: ['tasks', 'today', 'deviceTier'],
      emits: ['task-click'],
      setup: () => () => h('div'),
    }),
  };
});
vi.mock('@koyori-app/arc-vue/style.css', () => ({}));
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
  GET.mockReset();
  searchAll.value = {};
  media.wide = false;
  window.history.replaceState(null, '', '/');
});

function scheduleTask(id: string, project: { id: string; key: string }) {
  // 今日の締切 = 表示中の 2 週間に必ず入る
  return {
    id,
    seq_id: 1,
    seq_key: `${project.key}-1`,
    title: id,
    project: { ...project, name: project.key, is_personal: false },
    status: { id: 's-1', name: 'Todo', color: '#888', is_done_state: false },
    priority: 'Medium',
    progress_pct: 0,
    assignees: [],
    soft_deadline: new Date().toISOString(),
  };
}

const TASKS = [
  scheduleTask('t-eng', { id: 'p-eng', key: 'ENG' }),
  scheduleTask('t-ops', { id: 'p-ops', key: 'OPS' }),
];

/** 期間の無い課題（開始予定日も締切も無い）。プロジェクトの絞りが数にも効くかを見る */
const UNSCHEDULED = [
  { ...scheduleTask('u-eng', { id: 'p-eng', key: 'ENG' }), soft_deadline: null },
  { ...scheduleTask('u-ops-1', { id: 'p-ops', key: 'OPS' }), soft_deadline: null },
  { ...scheduleTask('u-ops-2', { id: 'p-ops', key: 'OPS' }), soft_deadline: null },
];

function mountPage({ unscheduledFailures = 0 } = {}) {
  let failuresLeft = unscheduledFailures;
  // 本物の API と同じく、project_id があればそのプロジェクトの課題だけを返す
  GET.mockImplementation(
    async (path: string, { params }: { params: { query: { project_id?: string[] } } }) => {
      const wanted = params.query.project_id;
      const scope = <T extends { project: { id: string } }>(list: T[]) =>
        wanted ? list.filter((task) => wanted.includes(task.project.id)) : list;
      if (path.endsWith('/schedule/unscheduled')) {
        if (failuresLeft > 0) {
          failuresLeft -= 1;
          return { error: { message: 'internal-error' } };
        }
        const tasks = scope(UNSCHEDULED);
        return { data: { total: tasks.length, tasks, members: [], next_cursor: null } };
      }
      return {
        data: {
          tasks: scope(TASKS),
          members: [],
          sprints: [],
          milestones: [],
          range: { from: '', to: '' },
          next_cursor: null,
        },
      };
    },
  );
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(Page, {
    global: { plugins: [[VueQueryPlugin, { queryClient }], createPinia()] },
  });
}

describe('テナントのガント', () => {
  it('プロジェクトで絞らず、テナントのすべてのプロジェクトのタスクを出す', async () => {
    const wrapper = mountPage();
    await flushPromises();

    expect(GET).toHaveBeenCalled();
    for (const [, { params }] of GET.mock.calls) {
      expect(params.query.project_id).toBeUndefined();
    }
    const bars = wrapper.getComponent({ name: 'GanttChart' }).props('tasks') as { id: string }[];
    expect(bars.map((bar) => bar.id).sort()).toEqual(['t-eng', 't-ops']);
    expect(wrapper.get('[data-testid="gantt-unscheduled"]').text()).toBe('期間なし 3 件');
  });

  it('ガントの図は端末の格を high にして、一覧や重ね文字の外で図だけが縦に動く形を作らせない', async () => {
    // arc-vue は deviceMemory の無い端末を low と判じ、図だけを内側のスクロール域に閉じ込める
    const wrapper = mountPage();
    await flushPromises();

    expect(wrapper.getComponent({ name: 'GanttChart' }).props('deviceTier')).toBe('high');
  });

  it('期間なしだけが読めない時は、図を出したまま期間なしの口で取り直せる', async () => {
    const wrapper = mountPage({ unscheduledFailures: 1 });
    await flushPromises();

    expect(wrapper.findComponent({ name: 'GanttChart' }).exists()).toBe(true);
    expect(wrapper.find('[data-testid="gantt-unscheduled"]').exists()).toBe(false);
    const unscheduledCalls = () =>
      GET.mock.calls.filter(([path]) => String(path).endsWith('/schedule/unscheduled')).length;
    expect(unscheduledCalls()).toBe(1);

    await wrapper.get('[data-testid="gantt-unscheduled-retry"]').trigger('click');
    await flushPromises();

    expect(unscheduledCalls()).toBe(2);
    expect(wrapper.get('[data-testid="gantt-unscheduled"]').text()).toBe('期間なし 3 件');
  });
});

describe('ガントで開いたタスクを URL に持つ（TASK-474）', () => {
  it('広い画面で棒を押すと URL に selected=KEY-N が載り、閉じると消える', async () => {
    media.wide = true;
    const wrapper = mountPage();
    await flushPromises();

    const chart = wrapper.getComponent({ name: 'GanttChart' });
    const bar = (chart.props('tasks') as { id: string }[]).find((b) => b.id === 't-ops')!;
    chart.vm.$emit('task-click', bar);
    await flushPromises();
    const overlay = () => wrapper.getComponent({ name: 'TaskDetailOverlay' });
    expect(overlay().props()).toMatchObject({ open: true, taskId: 'OPS-1' });
    expect(new URL(window.location.href).searchParams.get('selected')).toBe('OPS-1');

    overlay().vm.$emit('update:open', false);
    await flushPromises();
    expect(new URL(window.location.href).searchParams.has('selected')).toBe(false);
  });

  it('URL の selected で開くと、そのタスクをオーバーレイで開く', async () => {
    media.wide = true;
    searchAll.value = { selected: ['ENG-1'] };
    const wrapper = mountPage();
    await flushPromises();

    expect(wrapper.getComponent({ name: 'TaskDetailOverlay' }).props()).toMatchObject({
      open: true,
      projectKey: 'ENG',
      taskId: 'ENG-1',
    });
  });
});

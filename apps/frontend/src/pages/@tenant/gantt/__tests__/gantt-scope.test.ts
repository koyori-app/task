import { afterEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';

const { GET } = vi.hoisted(() => ({ GET: vi.fn() }));
vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({
    routeParams: { tenant: 'acme' },
    urlParsed: { searchAll: {} },
  }),
}));
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
      props: ['tasks', 'today'],
      emits: ['task-click'],
      setup: () => () => h('div'),
    }),
  };
});
vi.mock('@koyori-app/arc-vue/style.css', () => ({}));
vi.mock('@/components/tasks/TaskDetailOverlay.vue', async () => {
  const { defineComponent, h } = await import('vue');
  return { default: defineComponent({ name: 'TaskDetailOverlay', setup: () => () => h('div') }) };
});

import Page from '../+Page.vue';

enableAutoUnmount(afterEach);
afterEach(() => {
  GET.mockReset();
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

function mountPage() {
  // 本物の API と同じく、project_id があればそのプロジェクトの課題だけを返す
  GET.mockImplementation(
    async (path: string, { params }: { params: { query: { project_id?: string[] } } }) => {
      const wanted = params.query.project_id;
      const scope = <T extends { project: { id: string } }>(list: T[]) =>
        wanted ? list.filter((task) => wanted.includes(task.project.id)) : list;
      if (path.endsWith('/schedule/unscheduled')) {
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
});

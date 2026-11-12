import { afterEach, describe, expect, it, vi } from 'vitest';
import { type Ref, ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

const { GET, project } = vi.hoisted(() => ({
  GET: vi.fn(),
  // プロジェクトの id の解き方を試験ごとに決め、解けた後の値を試験から動かす
  project: {
    initialId: 'p-eng' as string | null,
    initialNotFound: false,
    projectId: null as Ref<string | null> | null,
    isProjectNotFound: null as Ref<boolean> | null,
  },
}));
vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({
    routeParams: { tenant: 'acme', projectKey: 'ENG' },
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
  useResolvedProjectId: () => {
    project.projectId = ref(project.initialId);
    project.isProjectNotFound = ref(project.initialNotFound);
    return {
      projectId: project.projectId,
      isProjectNotFound: project.isProjectNotFound,
      isResolving: ref(false),
      isError: ref(false),
    };
  },
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
  project.initialId = 'p-eng';
  project.initialNotFound = false;
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

function mountPage() {
  // 本物の API と同じく、project_id があればそのプロジェクトの課題だけを返す
  GET.mockImplementation(
    async (_path: string, { params }: { params: { query: { project_id?: string[] } } }) => {
      const wanted = params.query.project_id;
      return {
        data: {
          tasks: wanted ? TASKS.filter((task) => wanted.includes(task.project.id)) : TASKS,
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
  return mount(Page, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
}

describe('プロジェクトのガント', () => {
  it('そのプロジェクトのタスクだけを出し、他のプロジェクトのタスクを混ぜない', async () => {
    const wrapper = mountPage();
    await flushPromises();

    expect(GET).toHaveBeenCalled();
    for (const [, { params }] of GET.mock.calls) {
      expect(params.query.project_id).toEqual(['p-eng']);
    }
    const bars = wrapper.getComponent({ name: 'GanttChart' }).props('tasks') as { id: string }[];
    expect(bars.map((bar) => bar.id)).toEqual(['t-eng']);
  });

  it('プロジェクトの id が解けるまでは予定を引かず、解けたら project_id 付きで一度だけ引く', async () => {
    project.initialId = null;
    const wrapper = mountPage();
    await flushPromises();

    // 解ける前に引けば project_id の無い要求になり、テナント全体の予定が返る
    expect(GET).not.toHaveBeenCalled();
    expect(wrapper.findComponent({ name: 'GanttChart' }).exists()).toBe(false);

    project.projectId!.value = 'p-eng';
    await flushPromises();

    expect(GET).toHaveBeenCalledTimes(1);
    expect(GET.mock.calls[0]![1].params.query.project_id).toEqual(['p-eng']);
    const bars = wrapper.getComponent({ name: 'GanttChart' }).props('tasks') as { id: string }[];
    expect(bars.map((bar) => bar.id)).toEqual(['t-eng']);
  });

  it('プロジェクトが見つからない時は予定を引かず、見つからない旨を出す', async () => {
    project.initialId = null;
    project.initialNotFound = true;
    const wrapper = mountPage();
    await flushPromises();

    expect(GET).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('プロジェクトが見つかりません');
    expect(wrapper.findComponent({ name: 'GanttChart' }).exists()).toBe(false);
  });
});

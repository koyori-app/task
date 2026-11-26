import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reactive, ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';

type PageContextStub = {
  routeParams: Record<string, string>;
  urlParsed: { searchAll: Record<string, string[]> };
};
const { GET, page, media } = vi.hoisted(() => ({
  GET: vi.fn(),
  // 同じページへの遷移を試すため、vike と同じく反応的な pageContext を渡す
  page: { ctx: null as unknown as PageContextStub },
  // 既定は jsdom と同じ狭い画面。オーバーレイの試験だけ広い画面にする
  media: { wide: false },
}));
vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => page.ctx,
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
      props: ['tasks', 'today', 'deps', 'scrollContainer'],
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
beforeEach(() => {
  page.ctx = reactive({ routeParams: { tenant: 'acme' }, urlParsed: { searchAll: {} } });
});
afterEach(() => {
  GET.mockReset();
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

function mountPage({
  unscheduledFailures = 0,
  attachTo,
}: { unscheduledFailures?: number; attachTo?: HTMLElement } = {}) {
  let failuresLeft = unscheduledFailures;
  // 本物の API と同じく、project_id があればそのプロジェクトの課題だけを返す
  GET.mockImplementation(
    async (path: string, { params }: { params: { query: { project_id?: string[] } } }) => {
      const wanted = params.query.project_id;
      const scope = <T extends { project: { id: string } }>(list: T[]) =>
        wanted ? list.filter((task) => wanted.includes(task.project.id)) : list;
      if (path.endsWith('/schedule/relations')) {
        return { data: { relations: [], truncated: false } };
      }
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
    ...(attachTo ? { attachTo } : {}),
  });
}

describe('テナントのガント', () => {
  it('プロジェクトで絞らず、テナントのすべてのプロジェクトのタスクを出す', async () => {
    const wrapper = mountPage();
    await flushPromises();

    expect(wrapper.get('h1').text()).toBe('ガント');
    expect(wrapper.text()).not.toContain(
      'すべてのプロジェクトのタスクを、予定開始から目安の期限までの棒と、厳守の期限の菱形で見る',
    );
    expect(GET).toHaveBeenCalled();
    for (const [, { params }] of GET.mock.calls) {
      expect(params.query.project_id).toBeUndefined();
    }
    const bars = wrapper.getComponent({ name: 'GanttChart' }).props('tasks') as { id: string }[];
    expect(bars.map((bar) => bar.id).sort()).toEqual(['t-eng', 't-ops']);
    expect(wrapper.get('[data-testid="gantt-unscheduled"]').text()).toBe('期間なし 3 件');
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
    page.ctx.urlParsed = { searchAll: { selected: ['ENG-1'] } };
    const wrapper = mountPage();
    await flushPromises();

    expect(wrapper.getComponent({ name: 'TaskDetailOverlay' }).props()).toMatchObject({
      open: true,
      projectKey: 'ENG',
      taskId: 'ENG-1',
    });
  });

  it('同じページへの遷移や戻るで URL が変わると、開いているタスクと表示の切り替えを合わせる', async () => {
    media.wide = true;
    window.history.replaceState(null, '', '/acme/gantt?selected=ENG-1');
    page.ctx.urlParsed = { searchAll: { selected: ['ENG-1'] } };
    const wrapper = mountPage();
    await flushPromises();
    const overlay = () => wrapper.getComponent({ name: 'TaskDetailOverlay' });

    // サイドバーから selected の無い /acme/gantt へ
    window.history.replaceState(null, '', '/acme/gantt');
    page.ctx.urlParsed = { searchAll: {} };
    await flushPromises();
    expect(overlay().props('open')).toBe(false);
    expect(new URL(window.location.href).searchParams.has('selected')).toBe(false);

    // 「戻る」で selected と目盛りのある URL へ
    window.history.replaceState(null, '', '/acme/gantt?selected=OPS-1&scale=week');
    page.ctx.urlParsed = { searchAll: { selected: ['OPS-1'], scale: ['week'] } };
    await flushPromises();
    expect(overlay().props()).toMatchObject({ open: true, projectKey: 'OPS', taskId: 'OPS-1' });
    expect(wrapper.get('[data-testid="gantt-scale-week"]').attributes('aria-pressed')).toBe('true');
    // 戻った URL を古い状態で上書きしない
    const params = new URL(window.location.href).searchParams;
    expect(params.get('selected')).toBe('OPS-1');
    expect(params.get('scale')).toBe('week');
  });
});

describe('ガントの外のスクロール域と依存の切り替えの移り', () => {
  const relationCalls = () =>
    GET.mock.calls.filter(([path]) => String(path).endsWith('/schedule/relations')).length;

  it('頁を巻く main の中では、予定を読めた後に図の箱から辿った main を外のスクロール域に渡す', async () => {
    media.wide = true;
    // +Layout.vue の main と同じく縦に巻く祖先。予定を読む間は図の箱が組まれず、読めた後に組まれる
    const main = document.createElement('main');
    main.style.overflowY = 'auto';
    document.body.appendChild(main);
    try {
      const wrapper = mountPage({ attachTo: main });
      await flushPromises();

      expect(wrapper.getComponent({ name: 'GanttChart' }).props('scrollContainer')).toBe(main);
    } finally {
      main.remove();
    }
  });

  it('同じページの移りで deps が変われば、矢印の切り替えと関係の取得を合わせ、URL を古い deps で上書きしない', async () => {
    media.wide = true;
    window.history.replaceState(null, '', '/acme/gantt?deps=on');
    page.ctx.urlParsed = { searchAll: { deps: ['on'] } };
    const wrapper = mountPage();
    await flushPromises();
    // data-testid は switch の要素そのものか、その外に付く
    const toggle = () => {
      const el = wrapper.get('[data-testid="gantt-dependencies"]');
      return el.attributes('role') === 'switch' ? el : el.get('[role="switch"]');
    };
    expect(toggle().attributes('aria-checked')).toBe('true');
    const before = relationCalls();
    expect(before).toBeGreaterThan(0);

    // 「戻る」で deps=off と目盛りのある URL へ
    window.history.replaceState(null, '', '/acme/gantt?deps=off&scale=week');
    page.ctx.urlParsed = { searchAll: { deps: ['off'], scale: ['week'] } };
    await flushPromises();

    expect(toggle().attributes('aria-checked')).toBe('false');
    expect(wrapper.getComponent({ name: 'GanttChart' }).props('deps')).toEqual([]);
    expect(relationCalls()).toBe(before);
    const params = new URL(window.location.href).searchParams;
    expect(params.get('deps')).toBe('off');
    expect(params.get('scale')).toBe('week');
  });
});

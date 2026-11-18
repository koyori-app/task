import { afterEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';

// 依存の矢印と、狭い画面（1024px 未満）の依存の印。画面の幅は useMediaQuery を差し替えて決める
const { GET, page, screen } = vi.hoisted(() => ({
  GET: vi.fn(),
  page: { searchAll: {} as Record<string, string[]> },
  screen: { wide: false },
}));
vi.mock('@vueuse/core', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@vueuse/core')>();
  const { ref: vueRef } = await import('vue');
  return { ...actual, useMediaQuery: () => vueRef(screen.wide) };
});
vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({
    routeParams: { tenant: 'acme' },
    urlParsed: { searchAll: page.searchAll },
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
vi.mock('@koyori-app/arc-vue', async () => {
  const { defineComponent, h } = await import('vue');
  return {
    GanttChart: defineComponent({
      name: 'GanttChart',
      props: ['tasks', 'today', 'deps', 'scrollContainer', 'deviceTier'],
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
  page.searchAll = {};
  screen.wide = false;
  window.history.replaceState(null, '', '/');
});

// どの課題も今日に予定開始と目安の期限を置く（狭い画面の今日の週に必ず入る）
const today = new Date();
const day = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
function task(id: string, key: string, done = false) {
  return {
    id,
    seq_id: Number(key.split('-')[1]),
    seq_key: key,
    title: `題 ${key}`,
    project: { id: 'p-eng', key: 'ENG', name: 'Engineering', is_personal: false },
    status: { id: 's', name: done ? 'Done' : 'Todo', color: '#888', is_done_state: done },
    priority: 'Medium',
    progress_pct: 0,
    assignees: [],
    planned_start_on: day,
    soft_deadline: `${day}T12:00:00Z`,
  };
}
const TASKS = [
  task('a', 'ENG-1'),
  task('b', 'ENG-2'),
  task('c', 'ENG-3'),
  task('d', 'ENG-4', true),
];
// ENG-1 は未完了のまま ENG-2 を阻む（ENG-2 は今日始まるので待ちで遅れ）。済んだ ENG-4 は ENG-3 を阻む
const RELATIONS = [
  { from_task_id: 'a', to_task_id: 'b', type: 'blocks' },
  { from_task_id: 'd', to_task_id: 'c', type: 'blocks' },
];

function mountPage({ relationsFail = false } = {}) {
  GET.mockImplementation(async (path: string) => {
    if (path.endsWith('/schedule/relations')) {
      return relationsFail
        ? { error: { message: 'internal-error' } }
        : { data: { relations: RELATIONS, truncated: false } };
    }
    if (path.endsWith('/schedule/unscheduled')) {
      return { data: { total: 0, tasks: [], members: [], next_cursor: null } };
    }
    return {
      data: {
        tasks: TASKS,
        members: [],
        sprints: [],
        milestones: [],
        range: { from: '', to: '' },
        next_cursor: null,
      },
    };
  });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(Page, { global: { plugins: [[VueQueryPlugin, { queryClient }], createPinia()] } });
}

const relationCalls = () =>
  GET.mock.calls.filter(([path]) => String(path).endsWith('/schedule/relations'));

describe('狭い画面の依存の印', () => {
  it('待ちで遅れの行は阻む課題のキーと「待ち」を赤字で、関係のある他の行は「依存 N」を添える', async () => {
    const wrapper = mountPage();
    await flushPromises();

    const waiting = wrapper.findAll('[data-testid="gantt-dependency-waiting"]');
    expect(waiting.map((w) => w.text())).toEqual(['・ENG-1 待ち']);
    expect(waiting[0]!.classes()).toContain('text-destructive');
    expect(wrapper.findAll('[data-testid="gantt-dependency-count"]').map((c) => c.text())).toEqual([
      '・依存 1',
      '・依存 1',
      '・依存 1',
    ]);
    // 切り替えの名は狭い画面では「依存の印」
    expect(wrapper.get('[data-testid="gantt-dependencies"]').text()).toContain('依存の印');
  });

  it('URL で切ってあれば、関係を引かず印も出さぬ', async () => {
    page.searchAll = { deps: ['off'] };
    const wrapper = mountPage();
    await flushPromises();

    expect(relationCalls()).toHaveLength(0);
    expect(wrapper.find('[data-testid="gantt-dependency-waiting"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="gantt-dependency-count"]').exists()).toBe(false);
  });

  it('関係だけが読めぬ時は、図を出したまま再試行を出し、押すと取り直す', async () => {
    const wrapper = mountPage({ relationsFail: true });
    await flushPromises();

    expect(wrapper.findComponent({ name: 'GanttChart' }).exists()).toBe(true);
    expect(relationCalls()).toHaveLength(1);
    await wrapper.get('[data-testid="gantt-dependencies-retry"]').trigger('click');
    await flushPromises();
    expect(relationCalls()).toHaveLength(2);
  });
});

describe('広い画面の依存の矢印', () => {
  it('関係に状態を付けて図へ渡す（未完了が阻んで予定開始を過ぎれば待ちで遅れ、阻みが済めば済み）', async () => {
    screen.wide = true;
    const wrapper = mountPage();
    await flushPromises();

    const deps = wrapper.getComponent({ name: 'GanttChart' }).props('deps') as {
      blocker_task_id: string;
      blocked_task_id: string;
      state: string;
      note?: string;
    }[];
    expect(deps).toEqual([
      { blocker_task_id: 'a', blocked_task_id: 'b', state: 'delayed', note: 'ENG-1 待ち' },
      { blocker_task_id: 'd', blocked_task_id: 'c', state: 'done' },
    ]);
  });

  it('切り替えを切れば矢印を渡さぬ', async () => {
    screen.wide = true;
    page.searchAll = { deps: ['off'] };
    const wrapper = mountPage();
    await flushPromises();

    expect(wrapper.getComponent({ name: 'GanttChart' }).props('deps')).toEqual([]);
  });

  it('図に外のスクロール域を渡し、端末の格は決め打たぬ', async () => {
    screen.wide = true;
    const wrapper = mountPage();
    await flushPromises();

    const chart = wrapper.getComponent({ name: 'GanttChart' });
    // 試験の頁には縦に巻く祖先が無いので、頁そのもの（window）を巻く域にする
    expect(chart.props('scrollContainer')).toBe(window);
    expect(chart.props('deviceTier')).toBeUndefined();
  });

  it('狭い画面では矢印を描かず、行の添えに印を出す', async () => {
    const wrapper = mountPage();
    await flushPromises();

    expect(wrapper.getComponent({ name: 'GanttChart' }).props('deps')).toEqual([]);
  });
});

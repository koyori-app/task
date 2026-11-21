import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reactive, ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';

import { SCHEDULE_PATH } from '@/lib/calendar/schedule';
import { addDays, dateInZone } from '@/lib/calendar/timeline';
import { PAN_SETTLE_MS, PAN_SLOP_PX } from '@/lib/gantt/pan';
import { defaultFrom, viewTo } from '@/lib/gantt/view';

type PageContextStub = {
  routeParams: Record<string, string>;
  urlParsed: { searchAll: Record<string, string[]> };
};
const { GET, page } = vi.hoisted(() => ({
  GET: vi.fn(),
  page: { ctx: null as unknown as PageContextStub },
}));
vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => page.ctx,
}));
// 狭い画面（日の目盛りで 7 日。箱の幅は 0 なので 1 日 30px）
vi.mock('@vueuse/core', async (importOriginal) => {
  const { ref } = await import('vue');
  return {
    ...(await importOriginal<typeof import('@vueuse/core')>()),
    useMediaQuery: () => ref(false),
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
vi.mock('@koyori-app/arc-vue', async () => {
  const { defineComponent, h } = await import('vue');
  return {
    GanttChart: defineComponent({
      name: 'GanttChart',
      props: ['tasks', 'today', 'deps', 'scrollContainer', 'view'],
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
  window.history.replaceState(null, '', '/');
});

const TODAY = dateInZone(new Date(), Intl.DateTimeFormat().resolvedOptions().timeZone);
const FROM = defaultFrom(TODAY, 'day', true);

function rangeText(from: string) {
  const fmt = (date: string) => {
    const [y, m, d] = date.split('-');
    return `${y}/${Number(m)}/${Number(d)}`;
  };
  return `${fmt(from)} 〜 ${fmt(viewTo(from, 'day', true))}`;
}

const TASK = {
  id: 't-eng',
  seq_id: 1,
  seq_key: 'ENG-1',
  title: 't-eng',
  project: { id: 'p-eng', key: 'ENG', name: 'ENG', is_personal: false },
  status: { id: 's-1', name: 'Todo', color: '#888', is_done_state: false },
  priority: 'Medium',
  progress_pct: 0,
  assignees: [],
  soft_deadline: new Date().toISOString(),
};

const isSchedule = (path: string) => path === SCHEDULE_PATH;

function mountPage({ tasks = [TASK] }: { tasks?: unknown[] } = {}) {
  GET.mockImplementation(async (path: string) => {
    if (path.endsWith('/schedule/relations')) return { data: { relations: [], truncated: false } };
    if (path.endsWith('/schedule/unscheduled')) {
      return { data: { total: 0, tasks: [], members: [], next_cursor: null } };
    }
    return {
      data: {
        tasks,
        members: [],
        sprints: [],
        milestones: [],
        range: { from: '', to: '' },
        next_cursor: null,
      },
    };
  });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return mount(Page, {
    global: { plugins: [[VueQueryPlugin, { queryClient }], createPinia()] },
  });
}

const range = (wrapper: VueWrapper) => wrapper.get('[data-testid="gantt-range"]').text();
const panArea = (wrapper: VueWrapper) => wrapper.get('[data-testid="gantt-pan"]');
const groupHeader = (wrapper: VueWrapper) =>
  wrapper.get('[data-testid="gantt-body"] button[aria-expanded]');
const scheduleCalls = () => GET.mock.calls.filter(([path]) => isSchedule(String(path)));
const urlFrom = () => new URL(window.location.href).searchParams.get('from');
const settle = async () => {
  await new Promise((resolve) => setTimeout(resolve, PAN_SETTLE_MS + 50));
  await flushPromises();
};
const touch = (clientX: number) => ({
  pointerType: 'touch',
  isPrimary: true,
  pointerId: 1,
  clientX,
});

async function swipe(wrapper: VueWrapper, fromX: number, toX: number) {
  await panArea(wrapper).trigger('pointerdown', touch(fromX));
  await panArea(wrapper).trigger('pointermove', touch(toX));
  await panArea(wrapper).trigger('pointerup', touch(toX));
}

describe('ガントの横のスクロールとスワイプ（TASK-567）', () => {
  it('横の wheel で、範囲が 1 日の幅ごとに後ろへ動く', async () => {
    const wrapper = mountPage();
    await flushPromises();
    expect(range(wrapper)).toBe(rangeText(FROM));

    await panArea(wrapper).trigger('wheel', { deltaX: 65 });

    expect(range(wrapper)).toBe(rangeText(addDays(FROM, 2)));
  });

  it('縦が勝つ wheel では範囲が動かず、頁のスクロールを止めない', async () => {
    const wrapper = mountPage();
    await flushPromises();

    const event = new WheelEvent('wheel', {
      deltaX: 10,
      deltaY: 100,
      bubbles: true,
      cancelable: true,
    });
    panArea(wrapper).element.dispatchEvent(event);
    await flushPromises();

    expect(range(wrapper)).toBe(rangeText(FROM));
    expect(event.defaultPrevented).toBe(false);
  });

  it('送っている間は取得も URL の書き込みもせず、止まってから 1 回だけ行う', async () => {
    const wrapper = mountPage();
    await flushPromises();
    const before = scheduleCalls().length;

    await panArea(wrapper).trigger('wheel', { deltaX: 35 });
    await panArea(wrapper).trigger('wheel', { deltaX: 30 });
    expect(range(wrapper)).toBe(rangeText(addDays(FROM, 2)));
    expect(scheduleCalls().length).toBe(before);
    expect(urlFrom()).toBeNull();

    await settle();

    const moved = addDays(FROM, 2);
    expect(scheduleCalls().length).toBe(before + 1);
    expect(scheduleCalls().at(-1)![1].params.query).toMatchObject({
      from: moved,
      to: viewTo(moved, 'day', true),
    });
    expect(urlFrom()).toBe(moved);
  });

  it('取り直しの応答を待つ間も図を残し、読み込み中の表示で外さない', async () => {
    const wrapper = mountPage();
    await flushPromises();
    const base = GET.getMockImplementation()!;
    GET.mockImplementation((path: string, init: unknown) =>
      isSchedule(path) ? new Promise(() => {}) : base(path, init),
    );

    await wrapper.get('button[aria-label="次の期間"]').trigger('click');
    await flushPromises();

    expect(scheduleCalls().length).toBe(2);
    expect(wrapper.find('[data-testid="gantt-body"]').exists()).toBe(true);
    expect(wrapper.find('[aria-label="読み込み中"]').exists()).toBe(false);
  });

  it('タッチの横のスワイプで範囲が動き、続く click は見出しを畳まない', async () => {
    const wrapper = mountPage();
    await flushPromises();

    await swipe(wrapper, 200, 130);
    expect(range(wrapper)).toBe(rangeText(addDays(FROM, 2)));
    await groupHeader(wrapper).trigger('click');
    expect(groupHeader(wrapper).attributes('aria-expanded')).toBe('true');

    // 対照: 動かさずに離した後の click は畳む
    await swipe(wrapper, 200, 200);
    await groupHeader(wrapper).trigger('click');
    expect(groupHeader(wrapper).attributes('aria-expanded')).toBe('false');
  });

  it(`${PAN_SLOP_PX}px までの動きは押したものとみなし、越えればスワイプとみなす`, async () => {
    const wrapper = mountPage();
    await flushPromises();

    await swipe(wrapper, 200, 200 - PAN_SLOP_PX);
    await groupHeader(wrapper).trigger('click');
    expect(groupHeader(wrapper).attributes('aria-expanded')).toBe('false');

    await swipe(wrapper, 200, 200 - PAN_SLOP_PX - 1);
    await groupHeader(wrapper).trigger('click');
    expect(groupHeader(wrapper).attributes('aria-expanded')).toBe('false');
    expect(range(wrapper)).toBe(rangeText(FROM));
  });

  it('マウスのドラッグでは動かない', async () => {
    const wrapper = mountPage();
    await flushPromises();

    const mouse = (clientX: number) => ({ pointerType: 'mouse', isPrimary: true, clientX });
    await panArea(wrapper).trigger('pointerdown', mouse(200));
    await panArea(wrapper).trigger('pointermove', mouse(100));
    await panArea(wrapper).trigger('pointerup', mouse(100));

    expect(range(wrapper)).toBe(rangeText(FROM));
  });

  it('予定の無い期間でも、横の wheel で範囲を動かせる', async () => {
    const wrapper = mountPage({ tasks: [] });
    await flushPromises();
    expect(wrapper.find('[data-testid="gantt-empty"]').exists()).toBe(true);

    await panArea(wrapper).trigger('wheel', { deltaX: -30 });

    expect(range(wrapper)).toBe(rangeText(addDays(FROM, -1)));
  });

  it('送った後に「今日」を押すと既定の範囲へ戻り、URL の from が消える', async () => {
    const wrapper = mountPage();
    await flushPromises();
    await panArea(wrapper).trigger('wheel', { deltaX: 90 });
    await settle();
    expect(urlFrom()).toBe(addDays(FROM, 3));

    const todayButton = wrapper.findAll('button').find((b) => b.text() === '今日')!;
    await todayButton.trigger('click');
    await flushPromises();

    expect(range(wrapper)).toBe(rangeText(FROM));
    expect(urlFrom()).toBeNull();
  });
});

import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import GanttPage from '@/pages/@tenant/gantt/+Page.vue';
import type { components } from '@/generated/api';

type ScheduleTask = components['schemas']['ScheduleTask'];

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

// preview.ts で現在時刻は 2026-07-15（水）に固定。日の目盛りの表示は 7/6（月）〜 7/26 の 3 週
function pageContext(searchAll: Record<string, string[]> = {}) {
  return {
    urlPathname: '/tenant-123/gantt',
    routeParams: { tenant: 'tenant-123' },
    urlParsed: { searchAll },
  };
}

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_ENG = { id: 'proj-eng', key: 'ENG', name: 'Engineering', is_personal: false };
const PROJECT_DOC = { id: 'proj-doc', key: 'DOC', name: 'Docs', is_personal: false };
const TODO = { id: 's-todo', name: 'Todo', color: '#6b7280', is_done_state: false };
const DONE = { id: 's-done', name: 'Done', color: '#22c55e', is_done_state: true };
const members = [
  { id: 'u-alice', username: 'alice', avatar_url: null },
  { id: 'u-bob', username: 'bob', avatar_url: null },
  { id: 'u-carol', username: 'carol', avatar_url: null },
];

function task(seq: number, overrides: Partial<ScheduleTask>): ScheduleTask {
  return {
    id: `task-${seq}`,
    seq_id: seq,
    seq_key: `${(overrides.project ?? PROJECT_ENG).key}-${seq}`,
    title: `タスク ${seq}`,
    project: PROJECT_ENG,
    status: TODO,
    priority: 'Medium',
    progress_pct: 0,
    logged_minutes: 0,
    assignees: [],
    ...overrides,
  };
}

const sampleTasks: ScheduleTask[] = [
  task(1, {
    title: '認証フローの設計',
    planned_start_on: '2026-07-13',
    soft_deadline: '2026-07-22T00:00:00Z',
    hard_deadline: '2026-07-24T00:00:00Z',
    progress_pct: 40,
    assignees: [{ user_id: 'u-alice', role: 'primary' }],
  }),
  // 予定開始が無いので、目安の日の点で描く
  task(2, {
    title: 'API レビュー',
    soft_deadline: '2026-07-21T00:00:00Z',
    hard_deadline: '2026-07-23T00:00:00Z',
    assignees: [{ user_id: 'u-bob', role: 'primary' }],
  }),
  // 範囲の前から続き、完了
  task(3, {
    title: 'ログイン画面の実装',
    status: DONE,
    planned_start_on: '2026-07-08',
    soft_deadline: '2026-07-14T00:00:00Z',
    progress_pct: 100,
    assignees: [
      { user_id: 'u-carol', role: 'primary' },
      { user_id: 'u-alice', role: 'assignee' },
    ],
  }),
  // 完了しておらぬのに、今日（7/15）が目安の期限を過ぎた
  task(4, {
    title: '配布物の準備',
    planned_start_on: '2026-07-08',
    soft_deadline: '2026-07-14T00:00:00Z',
    hard_deadline: '2026-07-22T00:00:00Z',
    progress_pct: 60,
    assignees: [{ user_id: 'u-alice', role: 'primary' }],
  }),
  // 範囲の後ろへ続く
  task(6, {
    title: '移行の段取り',
    planned_start_on: '2026-07-27',
    soft_deadline: '2026-08-07T00:00:00Z',
    assignees: [{ user_id: 'u-bob', role: 'primary' }],
  }),
  // 0% で予定開始を過ぎた
  task(5, {
    title: 'リリースノート',
    project: PROJECT_DOC,
    planned_start_on: '2026-07-13',
    soft_deadline: '2026-07-17T00:00:00Z',
    assignees: [{ user_id: 'u-carol', role: 'primary' }],
  }),
];

const unscheduledTasks: ScheduleTask[] = [
  task(7, { title: '問い合わせの整理', assignees: [{ user_id: 'u-bob', role: 'primary' }] }),
  task(8, { title: '用語集', project: PROJECT_DOC }),
];

// 縦に長い図。狭い画面（試験の既定の幅 414px）の範囲 7/13〜7/19 に収める
const manyTasks: ScheduleTask[] = Array.from({ length: 20 }, (_, i) =>
  task(100 + i, {
    title: `並びの確かめ ${i + 1}`,
    planned_start_on: `2026-07-${13 + (i % 5)}`,
    soft_deadline: `2026-07-${15 + (i % 5)}T00:00:00Z`,
    progress_pct: (i * 17) % 100,
  }),
);

// 頁の main を巻く確かめ。行が main の高さを大きく越える数にする（範囲 7/13〜7/19 に収める）
const mainScrollTasks: ScheduleTask[] = Array.from({ length: 60 }, (_, i) =>
  task(200 + i, {
    title: `巻いた先の確かめ ${i + 1}`,
    planned_start_on: `2026-07-${13 + (i % 5)}`,
    soft_deadline: `2026-07-${15 + (i % 5)}T00:00:00Z`,
  }),
);

/**
 * arc-vue は `navigator.deviceMemory` と `hardwareConcurrency` で端末の格を判じる。
 * 物語の間だけ値を差し替え、終わったら外して元の値（Navigator.prototype の getter）に戻す。
 */
function emulateDevice(memory: number | undefined, cores: number) {
  Object.defineProperty(navigator, 'deviceMemory', { configurable: true, get: () => memory });
  Object.defineProperty(navigator, 'hardwareConcurrency', { configurable: true, get: () => cores });
  return () => {
    delete (navigator as { deviceMemory?: number }).deviceMemory;
    delete (navigator as { hardwareConcurrency?: number }).hardwareConcurrency;
  };
}

/**
 * 図の中のスクロール域と頁の両方を縦に送ってから、重ね文字の題の行と図の棒が同じ課題の行に
 * 並ぶことを確かめる。棒の上端と題の行の上端の差は、どの課題でも同じでなければならない。
 */
async function expectOverlayAlignedAfterScroll(canvasElement: HTMLElement) {
  const canvas = within(canvasElement);
  await expect(canvas.findByText('並びの確かめ 20')).resolves.toBeInTheDocument();
  await waitFor(() =>
    expect(canvasElement.querySelector('[data-task-id="task-100"]')).not.toBeNull(),
  );
  const scroller = canvasElement.querySelector<HTMLElement>('.koyori-gantt-scroll')!;
  // 図だけが縦に送れる形（内側のスクロール域）を残していないこと
  await expect(scroller.scrollHeight).toBeLessThanOrEqual(scroller.clientHeight + 1);
  scroller.scrollTop = 400;
  scroller.dispatchEvent(new Event('scroll'));
  window.scrollTo(0, 400);
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const deltas: number[] = [];
  for (const t of manyTasks) {
    const bar = canvasElement.querySelector(`[data-task-id="${t.id}"]`);
    if (!bar) continue;
    const title = canvas.getByText(t.title).closest('button')!;
    deltas.push(Math.round(bar.getBoundingClientRect().top - title.getBoundingClientRect().top));
  }
  await expect(deltas.length).toBe(manyTasks.length);
  // 棒は題の下（行の下寄せ）にあり、どの行でも同じ隔たり
  await expect(deltas[0]).toBeGreaterThan(0);
  await expect(new Set(deltas).size).toBe(1);
  window.scrollTo(0, 0);
}

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

function createMockFetch({
  tasks = sampleTasks,
  unscheduled = unscheduledTasks,
  relations = [],
}: {
  tasks?: ScheduleTask[];
  unscheduled?: ScheduleTask[];
  relations?: { from_task_id: string; to_task_id: string }[];
} = {}) {
  const original = globalThis.fetch;
  const membersOf = (list: ScheduleTask[]) => {
    const ids = new Set(list.flatMap((t) => t.assignees.map((a) => a.user_id)));
    return members.filter((m) => ids.has(m.id));
  };
  globalThis.fetch = fn().mockImplementation(async (req: Request | string) => {
    const url = new URL(typeof req === 'string' ? req : req.url, 'http://localhost');
    // arc の wasm など、API でない要求は本物の fetch に渡す（JSON を返すと wasm を読めない）
    if (!url.pathname.includes('/v1/')) return original(req);
    if (/\/v1\/tenants\/?$/.test(url.pathname)) {
      return jsonResponse([
        {
          id: TENANT_UUID,
          display_id: 'tenant-123',
          name: 'テストテナント',
          description: '',
          icon_url: '',
          owner_id: 'u-alice',
          require_2fa: false,
        },
      ]);
    }
    if (url.pathname.endsWith('/schedule/relations')) {
      return jsonResponse({
        relations: relations.map((r) => ({ ...r, type: 'blocks' })),
        truncated: false,
      });
    }
    if (url.pathname.endsWith('/schedule/unscheduled')) {
      return jsonResponse({
        total: unscheduled.length,
        tasks: unscheduled,
        members: membersOf(unscheduled),
        next_cursor: null,
      });
    }
    return jsonResponse({
      range: {
        from: url.searchParams.get('from'),
        to: url.searchParams.get('to'),
        timezone: url.searchParams.get('timezone'),
      },
      tasks,
      sprints: [],
      milestones: [],
      members: membersOf(tasks),
      next_cursor: null,
    });
  });
  return () => {
    globalThis.fetch = original;
  };
}

function withContext(searchAll: Record<string, string[]> = {}) {
  return () => ({
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
      provide(PAGE_CONTEXT_KEY, pageContext(searchAll));
    },
    template: '<story />',
  });
}

const meta = {
  title: 'Pages/Gantt',
  component: GanttPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'テナントのガント。左の一覧（キー・題・担当・厳守）と、arc が描く日ごとの目盛りと棒。GET /v1/tenants/{tenant_id}/schedule と /schedule/unscheduled を fetch モックで差し替え済み。',
      },
    },
  },
  beforeEach: () => {
    setActivePinia(createPinia());
  },
} satisfies Meta<typeof GanttPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const WithTasks: Story = {
  name: 'データあり（稲妻線 切れ）',
  decorators: [withContext()],
  beforeEach: () => createMockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('認証フローの設計')).resolves.toBeInTheDocument();
    // 今日（7/15）の週の前に一週を残した 3 週
    await expect(canvas.getByText('2026/7/6 〜 2026/7/26')).toBeInTheDocument();
    await expect(canvas.findByText('期間なし 2 件')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('目安 7/14 を過ぎた')).resolves.toBeInTheDocument();
    await expect(canvasElement.querySelector('.progress-point')).toBeNull();
  },
};

export const ProgressLineOn: Story = {
  name: '稲妻線 点け',
  decorators: [withContext({ line: ['on'] })],
  beforeEach: () => createMockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('認証フローの設計')).resolves.toBeInTheDocument();
    await expect(
      canvas.findByText('今日の時点の進みを結ぶ。左へ尖れば遅れ、右は先行'),
    ).resolves.toBeInTheDocument();
    // 遅れか先行の行の折れ目に点が出る
    await waitFor(() => expect(canvasElement.querySelector('.progress-point')).not.toBeNull());
  },
};

export const GroupByAssignee: Story = {
  name: '担当者で群れる',
  decorators: [withContext({ group: ['assignee'] })],
  beforeEach: () => createMockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 二人で持つ課題は二人の群れの両方に出る
    await expect(canvas.findAllByText('ログイン画面の実装')).resolves.toHaveLength(2);
  },
};

export const Empty: Story = {
  name: '空',
  decorators: [withContext()],
  beforeEach: () => createMockFetch({ tasks: [], unscheduled: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText('この期間に予定のあるタスクはありません'),
    ).resolves.toBeInTheDocument();
  },
};

export const LowDeviceScroll: Story = {
  name: '低性能の端末で縦に送る（狭い画面）',
  decorators: [withContext()],
  beforeEach: () => {
    const restoreDevice = emulateDevice(undefined, 2);
    const restoreFetch = createMockFetch({ tasks: manyTasks, unscheduled: [] });
    return () => {
      restoreFetch();
      restoreDevice();
    };
  },
  play: async ({ canvasElement }) => {
    await expectOverlayAlignedAfterScroll(canvasElement);
  },
};

export const HighDeviceScroll: Story = {
  name: '高性能の端末で縦に送る（狭い画面）',
  decorators: [withContext()],
  beforeEach: () => {
    const restoreDevice = emulateDevice(8, 8);
    const restoreFetch = createMockFetch({ tasks: manyTasks, unscheduled: [] });
    return () => {
      restoreFetch();
      restoreDevice();
    };
  },
  play: async ({ canvasElement }) => {
    await expectOverlayAlignedAfterScroll(canvasElement);
  },
};

// 依存の矢印の切り替えと凡例。関係の口を引き、切れば凡例の依存の項を消す。
// 狭い画面の依存の印は、画面の幅を差し替える単体試験（gantt-dependencies.test.ts）で縛る
export const DependencyToggle: Story = {
  name: '依存の矢印の切り替え',
  decorators: [withContext()],
  beforeEach: () =>
    createMockFetch({
      relations: [
        // 待ちで遅れ: 未完了の task-4 が、今日（7/15）より前に始まる task-1 を阻む
        { from_task_id: 'task-4', to_task_id: 'task-1' },
        // 済み: 完了した task-3 が task-5 を阻む
        { from_task_id: 'task-3', to_task_id: 'task-5' },
        // 普通: task-1 が、まだ始まらぬ task-2（目安 7/21）を阻む
        { from_task_id: 'task-1', to_task_id: 'task-2' },
      ],
    }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('認証フローの設計')).resolves.toBeInTheDocument();
    // 既定は点け: 凡例に依存・待ちで遅れ・済みが出る
    await expect(canvas.findByTestId('gantt-legend-dependency')).resolves.toBeInTheDocument();
    await expect(canvas.getByText('待ちで遅れ')).toBeInTheDocument();
    await userEvent.click(canvas.getByRole('switch', { name: '依存の矢印' }));
    await waitFor(() => expect(canvas.queryByTestId('gantt-legend-dependency')).toBeNull());
  },
};

// 頁の縦のスクロールは layout の main（overflow-y: auto）が持つ。低性能の端末（deviceMemory の無い
// Firefox・Safari を含む）では、図は外のスクロール域に見える行だけを描くので、main を巻いた先の行も
// 描かれねばならない（外の域を window に取り違えると、巻いた先が描かれない）
export const LowDeviceMainScroll: Story = {
  name: '低性能の端末で頁の main を巻く',
  decorators: [
    () => ({ template: '<main style="height: 480px; overflow-y: auto"><story /></main>' }),
    withContext(),
  ],
  beforeEach: () => {
    const restoreDevice = emulateDevice(undefined, 2);
    const restoreFetch = createMockFetch({ tasks: mainScrollTasks, unscheduled: [] });
    return () => {
      restoreFetch();
      restoreDevice();
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('巻いた先の確かめ 1')).resolves.toBeInTheDocument();
    const first = mainScrollTasks[0]!.id;
    const last = mainScrollTasks[mainScrollTasks.length - 1]!.id;
    const drawn = (id: string) => canvasElement.querySelector(`[data-task-id="${id}"]`) !== null;
    await waitFor(() => expect(drawn(first)).toBe(true));
    // 前の物語が window を巻いたままのことがある。window を頭へ戻し、その描き直しが済んでから見る
    // （window の位置が動くと、外の域を window に取り違えた形でも描き直されてしまう）
    window.scrollTo(0, 0);
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    // 端末は低性能と判じられ、見える行だけを描く（60 件のうち main に見える分だけ）
    await expect((navigator as { deviceMemory?: number }).deviceMemory).toBeUndefined();
    await expect(canvasElement.querySelectorAll('[data-task-id]').length).toBeLessThan(
      mainScrollTasks.length,
    );
    await expect(drawn(last)).toBe(false);
    // main を巻くと、main のスクロールを聞いて描き直す。arc-vue は描き直しを一つの rAF に寄せるので、
    // 二つの rAF の後にその場で見る（待ち直すと、試験の場の iframe の resize でも描き直されてしまい、
    // 外の域を window に取り違えた形でも通る）
    const main = canvasElement.querySelector('main')!;
    main.scrollTop = main.scrollHeight;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    await new Promise((resolve) => setTimeout(resolve, 0));
    await expect(drawn(last)).toBe(true);
  },
};

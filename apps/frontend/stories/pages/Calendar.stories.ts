import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import CalendarPage from '@/pages/@tenant/calendar/+Page.vue';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { components } from '@/generated/api';

type ScheduleTask = components['schemas']['ScheduleTask'];

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

// preview.ts で現在時刻は 2026-07-15（水）に固定。表示は 7/13（月）〜 7/26
const mockContext = {
  urlPathname: '/tenant-123/calendar',
  routeParams: { tenant: 'tenant-123' },
};

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
    soft_deadline: '2026-07-17T00:00:00Z',
    estimated_minutes: 600,
    assignees: [
      { user_id: 'u-alice', role: 'primary' },
      { user_id: 'u-bob', role: 'assignee' },
    ],
  }),
  task(2, {
    title: 'API レビュー',
    soft_deadline: '2026-07-21T00:00:00Z',
    assignees: [{ user_id: 'u-bob', role: 'primary' }],
  }),
  task(3, {
    title: 'ログイン画面の実装',
    status: DONE,
    planned_start_on: '2026-07-08',
    soft_deadline: '2026-07-14T00:00:00Z',
    assignees: [{ user_id: 'u-carol', role: 'primary' }],
  }),
  task(4, {
    title: '期限切れの修正',
    planned_start_on: '2026-07-09',
    soft_deadline: '2026-07-14T00:00:00Z',
    estimated_minutes: 120,
    assignees: [{ user_id: 'u-alice', role: 'primary' }],
  }),
  task(5, {
    title: 'リリースノート',
    project: PROJECT_DOC,
    planned_start_on: '2026-07-22',
    soft_deadline: '2026-07-31T00:00:00Z',
    estimated_minutes: 480,
    assignees: [{ user_id: 'u-carol', role: 'primary' }],
  }),
];

const overloadTasks: ScheduleTask[] = [
  task(11, {
    title: '移行スクリプト',
    planned_start_on: '2026-07-15',
    soft_deadline: '2026-07-16T00:00:00Z',
    estimated_minutes: 600,
    assignees: [{ user_id: 'u-alice', role: 'primary' }],
  }),
  task(12, {
    title: '負荷試験',
    planned_start_on: '2026-07-15',
    soft_deadline: '2026-07-16T00:00:00Z',
    estimated_minutes: 600,
    assignees: [{ user_id: 'u-alice', role: 'primary' }],
  }),
];

const sprint = (id: string, name: string, start_date: string, end_date: string) => ({
  id,
  project_id: PROJECT_ENG.id,
  name,
  goal: null,
  start_date,
  end_date,
  status: 'active' as const,
  created_by: 'u-alice',
  created_at: '2026-07-01T00:00:00Z',
  updated_at: '2026-07-01T00:00:00Z',
});

const sampleSprints = [
  sprint('sp-15', 'Sprint 15', '2026-07-06', '2026-07-19'),
  sprint('sp-16', 'Sprint 16', '2026-07-20', '2026-08-02'),
];
const sampleMilestones = [
  {
    id: 'ms-1',
    project_id: PROJECT_ENG.id,
    name: 'v1.0',
    description: null,
    due_date: '2026-07-24',
    created_by: 'u-alice',
    created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z',
  },
];

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

type MockOptions = {
  tasks?: ScheduleTask[];
  /** `include_overdue=true` のときだけ足す締切超過のタスク（期間より前に終わっている） */
  overdue?: ScheduleTask[];
  withPeriods?: boolean;
};

function createMockFetch({
  tasks: rangeTasks = sampleTasks,
  overdue = [],
  withPeriods = true,
}: MockOptions = {}) {
  const original = globalThis.fetch;
  globalThis.fetch = fn().mockImplementation(async (req: Request | string) => {
    const url = new URL(typeof req === 'string' ? req : req.url, 'http://localhost');
    if (/\/v1\/tenants\/?$/.test(url.pathname)) {
      return jsonResponse([
        {
          id: TENANT_UUID,
          display_id: mockContext.routeParams.tenant,
          name: 'テストテナント',
          description: '',
          icon_url: '',
          owner_id: 'u-alice',
          require_2fa: false,
        },
      ]);
    }
    const tasks =
      url.searchParams.get('include_overdue') === 'true' ? [...overdue, ...rangeTasks] : rangeTasks;
    const ids = new Set(tasks.flatMap((t) => t.assignees.map((a) => a.user_id)));
    return jsonResponse({
      range: {
        from: url.searchParams.get('from'),
        to: url.searchParams.get('to'),
        timezone: url.searchParams.get('timezone'),
      },
      tasks,
      sprints: withPeriods ? sampleSprints : [],
      milestones: withPeriods ? sampleMilestones : [],
      members: members.filter((m) => ids.has(m.id)),
      next_cursor: null,
    });
  });
  return () => {
    globalThis.fetch = original;
  };
}

const meta = {
  title: 'Pages/Calendar',
  component: CalendarPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'テナントのタイムライン。行がメンバー、横軸が 2 週間。GET /v1/tenants/{tenant_id}/schedule を fetch モックで差し替え済み。',
      },
    },
  },
  decorators: [
    () => ({
      setup() {
        const queryClient = new QueryClient({
          defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
        });
        provide(VUE_QUERY_CLIENT, queryClient);
        provide(PAGE_CONTEXT_KEY, mockContext);
      },
      // 本番では +Layout が用意している（山の内訳の Tooltip が使う）
      components: { TooltipProvider },
      template: '<TooltipProvider :delay-duration="0"><story /></TooltipProvider>',
    }),
  ],
} satisfies Meta<typeof CalendarPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const WithTasks: Story = {
  name: 'データあり',
  beforeEach: () => createMockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findAllByRole('link', { name: /認証フローの設計/ })).resolves.toHaveLength(
      2,
    );
    await expect(canvas.findByText('Sprint 15')).resolves.toBeInTheDocument();
    await expect(canvas.getByText('2026/7/13 〜 2026/7/26')).toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: '空',
  beforeEach: () => createMockFetch({ tasks: [], withPeriods: false }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('この期間のタスクはありません')).resolves.toBeInTheDocument();
  },
};

export const Overloaded: Story = {
  name: '超過あり',
  beforeEach: () => createMockFetch({ tasks: overloadTasks, withPeriods: false }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByLabelText('2026-07-15 の残り 10h（上限 6h を超過）'),
    ).resolves.toBeInTheDocument();
  },
};

export const Overdue: Story = {
  name: '締切超過あり',
  beforeEach: () =>
    createMockFetch({
      tasks: [],
      overdue: [
        task(21, {
          title: '先週締切の残り',
          planned_start_on: '2026-07-01',
          soft_deadline: '2026-07-03T00:00:00Z',
          estimated_minutes: 300,
          logged_minutes: 60,
          assignees: [{ user_id: 'u-bob', role: 'primary' }],
        }),
        // 開始予定日が無いので積まれない。carol の行を空のまま出さない
        task(22, {
          title: '先週締切の点',
          soft_deadline: '2026-07-03T00:00:00Z',
          estimated_minutes: 300,
          assignees: [{ user_id: 'u-carol', role: 'primary' }],
        }),
      ],
      withPeriods: false,
    }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 期間より前に締切を過ぎた残工数（5h − 1h）を今日に全部積む。帯は期間外なので出ない
    await expect(canvas.findByLabelText('2026-07-15 の残り 4h')).resolves.toBeInTheDocument();
    await expect(canvas.queryByRole('link', { name: /先週締切の残り/ })).toBeNull();
    await expect(canvasElement.querySelector('[data-member-row="u-bob"]')).not.toBeNull();
    await expect(canvasElement.querySelector('[data-member-row="u-carol"]')).toBeNull();
  },
};

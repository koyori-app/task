import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import GanttPage from '@/pages/@tenant/gantt/+Page.vue';
import type { components } from '@/generated/api';

type ScheduleTask = components['schemas']['ScheduleTask'];

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

// preview.ts で現在時刻は 2026-07-15（水）に固定。日の目盛りの表示は 7/13（月）〜 8/2 の 3 週
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

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

function createMockFetch({
  tasks = sampleTasks,
  unscheduled = unscheduledTasks,
}: { tasks?: ScheduleTask[]; unscheduled?: ScheduleTask[] } = {}) {
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
    await expect(canvas.getByText('2026/7/13 〜 2026/8/2')).toBeInTheDocument();
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

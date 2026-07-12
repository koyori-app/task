import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import TaskTablePage from '@/pages/@tenant/projects/@projectKey/tasks/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/tenant-123/projects/ENG/tasks',
  routeParams: { tenant: 'tenant-123', projectKey: 'ENG' },
};

const jsonResponse = (data: unknown) =>
  new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });

const sampleProjects = [
  {
    id: 'proj-eng',
    key: 'ENG',
    name: 'エンジニアリング',
    description: '',
    tenant_id: 'tenant-123',
    is_personal: false,
  },
];

const sampleStatuses = [
  {
    id: 's-backlog',
    name: 'Backlog',
    color: '#94a3b8',
    position: 0,
    is_default: true,
    is_done_state: false,
    project_id: 'proj-eng',
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: 's-progress',
    name: 'In Progress',
    color: '#3b82f6',
    position: 1,
    is_default: false,
    is_done_state: false,
    project_id: 'proj-eng',
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: 's-review',
    name: 'In Review',
    color: '#8b5cf6',
    position: 2,
    is_default: false,
    is_done_state: false,
    project_id: 'proj-eng',
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: 's-done',
    name: 'Done',
    color: '#22c55e',
    position: 3,
    is_default: false,
    is_done_state: true,
    project_id: 'proj-eng',
    created_at: '2026-01-01T00:00:00Z',
  },
];

/** Chromatic 日次差分揺れ防止の固定ユーザー */
const mockUsers = {
  alpha: {
    id: '11111111-1111-4111-8111-111111111101',
    username: '田中太郎',
    avatar_url: null,
  },
  beta: {
    id: '11111111-1111-4111-8111-111111111102',
    username: '佐藤花子',
    avatar_url: null,
  },
  gamma: {
    id: '11111111-1111-4111-8111-111111111103',
    username: '鈴木一郎',
    avatar_url: null,
  },
  delta: {
    id: '11111111-1111-4111-8111-111111111104',
    username: '高橋美咲',
    avatar_url: null,
  },
  epsilon: {
    id: '11111111-1111-4111-8111-111111111105',
    username: '伊藤健太',
    avatar_url: null,
  },
} as const;

const assignee = (user: (typeof mockUsers)[keyof typeof mockUsers]) => ({
  role: 'assignee',
  user,
});

const sampleTasks = {
  tasks: [
    {
      id: 'task-1',
      seq_id: 1,
      title: 'OAuth 対応を実装する',
      priority: 'High' as const,
      status_id: 's-progress',
      project_id: 'proj-eng',
      soft_deadline: '2026-07-02T00:00:00Z',
      hard_deadline: null,
      is_archived: false,
      progress_pct: 0,
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-15T00:00:00Z',
      created_by: mockUsers.alpha,
      assignees: [assignee(mockUsers.alpha), assignee(mockUsers.beta), assignee(mockUsers.gamma)],
    },
    {
      id: 'task-2',
      seq_id: 2,
      title: 'ログイン画面の UI 実装',
      priority: 'Medium' as const,
      status_id: 's-review',
      project_id: 'proj-eng',
      soft_deadline: '2026-06-29T00:00:00Z',
      hard_deadline: null,
      is_archived: false,
      progress_pct: 0,
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-15T00:00:00Z',
      created_by: mockUsers.alpha,
      assignees: [assignee(mockUsers.beta)],
    },
    {
      id: 'task-3',
      seq_id: 3,
      title: 'DB スキーマ設計',
      priority: 'Critical' as const,
      status_id: 's-done',
      project_id: 'proj-eng',
      soft_deadline: null,
      hard_deadline: null,
      is_archived: false,
      progress_pct: 100,
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-15T00:00:00Z',
      created_by: mockUsers.alpha,
      assignees: [assignee(mockUsers.gamma)],
    },
    {
      id: 'task-4',
      seq_id: 4,
      title: '通知メール送信機能',
      priority: 'Low' as const,
      status_id: 's-progress',
      project_id: 'proj-eng',
      soft_deadline: '2026-07-14T00:00:00Z',
      hard_deadline: null,
      is_archived: false,
      progress_pct: 0,
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-15T00:00:00Z',
      created_by: mockUsers.alpha,
      assignees: [],
    },
    {
      id: 'task-5',
      seq_id: 5,
      title: 'タスク一覧 API の実装',
      priority: 'Medium' as const,
      status_id: 's-backlog',
      project_id: 'proj-eng',
      soft_deadline: null,
      hard_deadline: null,
      is_archived: false,
      progress_pct: 0,
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-15T00:00:00Z',
      created_by: mockUsers.alpha,
      assignees: [assignee(mockUsers.alpha)],
    },
    {
      id: 'task-6',
      seq_id: 6,
      title: 'チーム全体ミーティング調整',
      priority: 'Low' as const,
      status_id: 's-backlog',
      project_id: 'proj-eng',
      soft_deadline: null,
      hard_deadline: null,
      is_archived: false,
      progress_pct: 0,
      created_at: '2026-06-01T00:00:00Z',
      updated_at: '2026-06-15T00:00:00Z',
      created_by: mockUsers.alpha,
      assignees: [
        assignee(mockUsers.alpha),
        assignee(mockUsers.beta),
        assignee(mockUsers.gamma),
        assignee(mockUsers.delta),
        assignee(mockUsers.epsilon),
      ],
    },
  ],
  total: 6,
};

/**
 * fetch モックで全 API エンドポイントを差し替える
 */
function createMockFetch(
  overrides: {
    projects?: typeof sampleProjects;
    statuses?: typeof sampleStatuses;
    tasks?: { tasks: unknown[]; total: number };
    rejectAll?: boolean;
    hang?: boolean;
  } = {},
) {
  const original = globalThis.fetch;
  globalThis.fetch = fn().mockImplementation(async (req: Request) => {
    if (overrides.rejectAll) {
      throw new TypeError('Failed to fetch');
    }
    if (overrides.hang) {
      return new Promise(() => {});
    }
    const url = typeof req === 'string' ? req : req.url;
    if (
      url.includes('/v1/tenants/') &&
      url.includes('/projects') &&
      !url.includes('/tasks') &&
      !url.includes('/statuses')
    ) {
      return jsonResponse(overrides.projects ?? sampleProjects);
    }
    if (url.includes('/statuses')) {
      return jsonResponse(overrides.statuses ?? sampleStatuses);
    }
    if (url.includes('/tasks')) {
      return jsonResponse(overrides.tasks ?? sampleTasks);
    }
    return jsonResponse({});
  });
  return () => {
    globalThis.fetch = original;
  };
}

function mockFetch() {
  return createMockFetch();
}

const meta = {
  title: 'Pages/TaskTable',
  component: TaskTablePage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクトタスク一覧の TanStack Table ビュー。fetch モックで全 API エンドポイントを差し替え。',
      },
    },
  },
  decorators: [
    () => ({
      setup() {
        const queryClient = new QueryClient({
          defaultOptions: {
            queries: { retry: false, gcTime: 0, staleTime: 0 },
            mutations: { retry: false },
          },
        });
        provide(VUE_QUERY_CLIENT, queryClient);
        provide(PAGE_CONTEXT_KEY, mockContext);
      },
      template: '<story />',
    }),
  ],
} satisfies Meta<typeof TaskTablePage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: 'タスクあり',
  beforeEach: mockFetch,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 既存タスク
    await expect(canvas.findByText('OAuth 対応を実装する')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('ログイン画面の UI 実装')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('ENG-1')).resolves.toBeInTheDocument();
    // 新規タスク
    await expect(canvas.findByText('チーム全体ミーティング調整')).resolves.toBeInTheDocument();
    // 複数担当者オーバーフロー表示（task-6 は5名, maxDisplay=3 → overflowCount=2 → 他2名）
    await expect(canvas.findByText(/他2名/)).resolves.toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: 'タスクなし',
  beforeEach: () => createMockFetch({ tasks: { tasks: [], total: 0 } }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクが見つかりません')).resolves.toBeInTheDocument();
  },
};

export const ProjectNotFound: Story = {
  name: 'プロジェクトなし',
  decorators: [
    () => ({
      setup() {
        const queryClient = new QueryClient({
          defaultOptions: {
            queries: { retry: false, gcTime: 0, staleTime: 0 },
            mutations: { retry: false },
          },
        });
        provide(VUE_QUERY_CLIENT, queryClient);
        provide(PAGE_CONTEXT_KEY, {
          urlPathname: '/tenant-123/projects/UNKNOWN/tasks',
          routeParams: { tenant: 'tenant-123', projectKey: 'UNKNOWN' },
        });
      },
      template: '<story />',
    }),
  ],
  beforeEach: mockFetch,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('プロジェクトが見つかりません')).resolves.toBeInTheDocument();
  },
};

export const ApiError: Story = {
  name: 'API エラー',
  beforeEach: () => createMockFetch({ rejectAll: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクの読み込みに失敗しました')).resolves.toBeInTheDocument();
  },
};

export const Loading: Story = {
  name: 'ロード中',
  beforeEach: () => createMockFetch({ hang: true }),
};

export const Sorting: Story = {
  name: 'ソート操作',
  beforeEach: mockFetch,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    const titleHeader = await canvas.findByRole('button', { name: /タイトル/ });
    await user.click(titleHeader);
    await expect(canvas.findByText('OAuth 対応を実装する')).resolves.toBeInTheDocument();
  },
};

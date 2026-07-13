import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import TaskDetailPage from '@/pages/@tenant/projects/@projectKey/tasks/@taskId/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/tenant-123/projects/ENG/tasks/ENG-1',
  routeParams: { tenant: 'tenant-123', projectKey: 'ENG', taskId: 'ENG-1' },
};

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const sampleTenants = (displayId: string) => [
  {
    id: TENANT_UUID,
    display_id: displayId,
    name: 'テストテナント',
    description: '',
    icon_url: '',
    owner_id: '00000000-0000-0000-0000-000000000002',
    require_2fa: false,
  },
];

const isListTenantsUrl = (url: string) => {
  try {
    const pathname = new URL(url, 'http://localhost').pathname;
    return /\/v1\/tenants\/?$/.test(pathname);
  } catch {
    return /\/v1\/tenants\/?(?:\?|$)/.test(url) && !/\/v1\/tenants\/[^/?]/.test(url);
  }
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

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
} as const;

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
    id: 's-done',
    name: 'Done',
    color: '#22c55e',
    position: 2,
    is_default: false,
    is_done_state: true,
    project_id: 'proj-eng',
    created_at: '2026-01-01T00:00:00Z',
  },
];

const sampleTaskDetail = {
  id: 'task-1',
  seq_id: 1,
  title: 'OAuth 対応を実装する',
  description: 'OIDC フローとセッション管理を実装する。',
  priority: 'High' as const,
  status_id: 's-progress',
  project_id: 'proj-eng',
  soft_deadline: '2026-07-02T00:00:00Z',
  hard_deadline: null,
  is_archived: false,
  progress_pct: 30,
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-15T00:00:00Z',
  created_by: mockUsers.alpha,
  assignees: [
    { role: 'assignee', user: mockUsers.alpha },
    { role: 'assignee', user: mockUsers.beta },
  ],
  custom_field_values: [],
};

type MockOptions = {
  task?: typeof sampleTaskDetail | null;
  rejectAll?: boolean;
  rejectTenantsList?: boolean;
  rejectPut?: number;
  hang?: boolean;
  onPut?: (body: unknown) => void;
};

function createMockFetch(overrides: MockOptions = {}) {
  const original = globalThis.fetch;
  globalThis.fetch = fn().mockImplementation(async (req: Request) => {
    const url = typeof req === 'string' ? req : req.url;
    if (isListTenantsUrl(url)) {
      if (overrides.rejectTenantsList) {
        return jsonResponse({ message: 'server error' }, 500);
      }
      return jsonResponse(sampleTenants(mockContext.routeParams.tenant));
    }
    if (overrides.rejectAll) throw new TypeError('Failed to fetch');
    if (overrides.hang) return new Promise(() => {});

    const method = typeof req === 'string' ? 'GET' : req.method;
    if (
      url.includes('/v1/tenants/') &&
      url.includes('/projects') &&
      !url.includes('/tasks') &&
      !url.includes('/statuses')
    ) {
      return jsonResponse(sampleProjects);
    }
    if (url.includes('/statuses')) {
      return jsonResponse(sampleStatuses);
    }
    if (method === 'PUT' && url.includes('/tasks/')) {
      if (overrides.rejectPut) {
        return jsonResponse({ message: 'update failed' }, overrides.rejectPut);
      }
      const body = await req.json();
      overrides.onPut?.(body);
      return jsonResponse({
        ...sampleTaskDetail,
        status_id: (body as { status_id?: string }).status_id ?? sampleTaskDetail.status_id,
      });
    }
    if (url.includes('/tasks/')) {
      if (overrides.task === null) {
        return jsonResponse({ message: 'not-found' }, 404);
      }
      return jsonResponse(overrides.task ?? sampleTaskDetail);
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

function storyDecorator(
  context: { urlPathname: string; routeParams: Record<string, string> } = mockContext,
) {
  return () => ({
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: {
          queries: { retry: false, gcTime: 0, staleTime: 0 },
          mutations: { retry: false },
        },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
      provide(PAGE_CONTEXT_KEY, context);
    },
    template: '<story />',
  });
}

const meta = {
  title: 'Pages/TaskDetail',
  component: TaskDetailPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'タスク詳細ハブ（増分1）。GET 表示・ステータス変更・loading/404/error を fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof TaskDetailPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: '詳細表示',
  beforeEach: mockFetch,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'OAuth 対応を実装する' }),
    ).resolves.toBeInTheDocument();
    await expect(canvas.findByText('ENG-1')).resolves.toBeInTheDocument();
    await expect(
      canvas.findByText('OIDC フローとセッション管理を実装する。'),
    ).resolves.toBeInTheDocument();
    await expect(canvas.findByText('田中太郎')).resolves.toBeInTheDocument();
  },
};

export const NotFound: Story = {
  name: 'タスクなし',
  beforeEach: () => createMockFetch({ task: null }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクが見つかりません')).resolves.toBeInTheDocument();
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

export const TenantResolveError: Story = {
  name: 'テナント解決エラー',
  beforeEach: () => createMockFetch({ rejectTenantsList: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクの読み込みに失敗しました')).resolves.toBeInTheDocument();
    expect(canvas.queryByText('タスクが見つかりません')).toBeNull();
  },
};

export const Loading: Story = {
  name: 'ロード中',
  beforeEach: () => createMockFetch({ hang: true }),
};

export const StatusChange: Story = {
  name: 'ステータス変更',
  beforeEach: () => createMockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(
      canvas.findByRole('heading', { name: 'OAuth 対応を実装する' }),
    ).resolves.toBeInTheDocument();

    const select = await canvas.findByRole('combobox', { name: 'ステータス' });
    await user.selectOptions(select, 's-done');
    await expect(select).toHaveValue('s-done');
  },
};

export const StatusChangeFailure500: Story = {
  name: 'ステータス変更失敗（500）',
  beforeEach: () => createMockFetch({ rejectPut: 500 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(
      canvas.findByRole('heading', { name: 'OAuth 対応を実装する' }),
    ).resolves.toBeInTheDocument();

    const select = await canvas.findByRole('combobox', { name: 'ステータス' });
    await expect(select).toHaveValue('s-progress');
    await user.selectOptions(select, 's-done');
    await expect(canvas.findByText('ステータスの更新に失敗しました')).resolves.toBeInTheDocument();
    await expect(select).toHaveValue('s-progress');
  },
};

export const StatusChangeFailure413: Story = {
  name: 'ステータス変更失敗（413）',
  beforeEach: () => createMockFetch({ rejectPut: 413 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(
      canvas.findByRole('heading', { name: 'OAuth 対応を実装する' }),
    ).resolves.toBeInTheDocument();

    const select = await canvas.findByRole('combobox', { name: 'ステータス' });
    await expect(select).toHaveValue('s-progress');
    await user.selectOptions(select, 's-done');
    await expect(canvas.findByText('ステータスの更新に失敗しました')).resolves.toBeInTheDocument();
    await expect(select).toHaveValue('s-progress');
  },
};

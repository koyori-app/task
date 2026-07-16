import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import MyTasksPage from '@/pages/@tenant/my-tasks/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/tenant-123/my-tasks',
  routeParams: { tenant: 'tenant-123' },
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

const sampleTasks = [
  {
    id: 'task-1',
    seq_key: 'FE-1',
    title: '仕様書のレビュー',
    priority: 'high',
    soft_deadline: '2026-06-20T00:00:00Z',
    hard_deadline: null,
    is_personal: false,
    project: { id: 'proj-1', name: 'フロントエンド', key: 'FE', is_personal: false },
    status: { id: 's1', name: 'In Progress', color: '#3b82f6', is_done_state: false },
  },
  {
    id: 'task-2',
    seq_key: 'BE-5',
    title: 'APIのドキュメント作成',
    priority: 'medium',
    soft_deadline: null,
    hard_deadline: null,
    is_personal: false,
    project: { id: 'proj-2', name: 'バックエンド', key: 'BE', is_personal: false },
    status: { id: 's2', name: 'Todo', color: '#6b7280', is_done_state: false },
  },
  {
    id: 'task-3',
    seq_key: 'P-1',
    title: '個人メモ',
    priority: 'low',
    soft_deadline: null,
    hard_deadline: null,
    is_personal: true,
    project: { id: 'proj-personal', name: '個人 Inbox', key: 'P', is_personal: true },
    status: { id: 's3', name: 'Todo', color: '#6b7280', is_done_state: false },
  },
];

type MockOptions = {
  tasks?: typeof sampleTasks;
  rejectTasks?: boolean;
  rejectTenantsList?: boolean;
  hang?: boolean;
};

function createMockFetch(overrides: MockOptions = {}) {
  const original = globalThis.fetch;
  const fetchSpy = fn().mockImplementation(async (req: Request | string) => {
    const url = typeof req === 'string' ? req : req.url;
    if (isListTenantsUrl(url)) {
      if (overrides.rejectTenantsList) {
        return jsonResponse({ message: 'server error' }, 500);
      }
      return jsonResponse(sampleTenants(mockContext.routeParams.tenant));
    }
    if (overrides.rejectTasks) throw new TypeError('Failed to fetch');
    if (overrides.hang) return new Promise<Response>(() => {});
    return jsonResponse({ tasks: overrides.tasks ?? sampleTasks });
  });
  globalThis.fetch = fetchSpy;
  return {
    fetchSpy,
    restore: () => {
      globalThis.fetch = original;
    },
  };
}

let activeMock: ReturnType<typeof createMockFetch> | null = null;

function mockDecoratorBeforeEach(overrides: MockOptions = {}) {
  return () => {
    activeMock = createMockFetch(overrides);
    return () => {
      activeMock?.restore();
      activeMock = null;
    };
  };
}

const meta = {
  title: 'Pages/MyTasks',
  component: MyTasksPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'テナント横断のタスク一覧ページ。fetch モックで apiClient を差し替え済み（GET /v1/tenants で display_id → UUID を解決する）。',
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
} satisfies Meta<typeof MyTasksPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const WithTasks: Story = {
  name: 'タスクあり（個人 + プロジェクト）',
  beforeEach: mockDecoratorBeforeEach(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('仕様書のレビュー')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('APIのドキュメント作成')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('個人メモ')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('個人 Inbox')).resolves.toBeInTheDocument();
  },
};

export const ResolvesTenantUuid: Story = {
  name: 'テナント解決（display_id → UUID）',
  beforeEach: mockDecoratorBeforeEach(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('仕様書のレビュー')).resolves.toBeInTheDocument();

    // 回帰ガード: タスク API のパスには route param（display_id）ではなく解決済み UUID を使う。
    // display_id をそのまま渡すと backend が UUID parse エラー（400）を返す（#(この修正) で実発生）。
    const calledUrls = (activeMock!.fetchSpy.mock.calls as [Request | string][]).map(([req]) =>
      typeof req === 'string' ? req : req.url,
    );
    const taskUrls = calledUrls.filter((url) => url.includes('/users/me/tasks'));
    await expect(taskUrls.length).toBeGreaterThan(0);
    for (const url of taskUrls) {
      await expect(url).toContain(`/v1/tenants/${TENANT_UUID}/users/me/tasks`);
      await expect(url).not.toContain('/v1/tenants/tenant-123/');
    }
  },
};

export const Empty: Story = {
  name: 'タスクなし',
  beforeEach: mockDecoratorBeforeEach({ tasks: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクがありません')).resolves.toBeInTheDocument();
  },
};

export const ApiError: Story = {
  name: 'API エラー',
  beforeEach: mockDecoratorBeforeEach({ rejectTasks: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクの読み込みに失敗しました')).resolves.toBeInTheDocument();
  },
};

export const TenantResolveError: Story = {
  name: 'テナント解決エラー',
  beforeEach: mockDecoratorBeforeEach({ rejectTenantsList: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクの読み込みに失敗しました')).resolves.toBeInTheDocument();
    await expect(canvas.queryByText('タスクがありません')).toBeNull();
  },
};

export const Loading: Story = {
  name: 'ロード中',
  beforeEach: mockDecoratorBeforeEach({ hang: true }),
};

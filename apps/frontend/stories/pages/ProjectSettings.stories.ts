import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fireEvent, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import ProjectSettingsPage from '@/pages/@tenant/projects/@projectKey/settings/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/tenant-123/projects/ALPHA/settings',
  routeParams: { tenant: 'tenant-123', projectKey: 'ALPHA' },
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

const sampleProject = {
  id: '00000000-0000-4000-8000-000000000010',
  tenant_id: TENANT_UUID,
  name: 'Team Alpha',
  description: 'Shared project',
  key: 'ALPHA',
  is_personal: false,
  icon_emoji: '🎨',
  icon_url: null,
  personal_owner_id: null,
};

const sampleStatuses = [
  {
    id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1',
    project_id: sampleProject.id,
    name: 'Todo',
    color: '#64748b',
    position: 0,
    is_default: true,
    is_done_state: false,
    created_at: '2026-07-18T00:00:00Z',
  },
  {
    id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2',
    project_id: sampleProject.id,
    name: 'レビュー中',
    color: '#f59e0b',
    position: 1,
    is_default: false,
    is_done_state: false,
    created_at: '2026-07-18T00:00:00Z',
  },
  {
    id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3',
    project_id: sampleProject.id,
    name: 'Done',
    color: '#22c55e',
    position: 2,
    is_default: false,
    is_done_state: true,
    created_at: '2026-07-18T00:00:00Z',
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

type MockOptions = {
  rejectWrite?: number;
  rejectStatuses?: boolean;
  hangStatuses?: boolean;
  noProject?: boolean;
  statuses?: typeof sampleStatuses;
};

let fetchSpy: ReturnType<typeof fn> | null = null;

function mockFetch(overrides: MockOptions = {}) {
  return () => {
    const original = globalThis.fetch;
    let currentStatuses = structuredClone(overrides.statuses ?? sampleStatuses);
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      if (isListTenantsUrl(url)) {
        return jsonResponse(sampleTenants(mockContext.routeParams.tenant));
      }
      if (pathname.endsWith('/statuses')) {
        if (overrides.hangStatuses) return new Promise<Response>(() => {});
        if (overrides.rejectStatuses) return jsonResponse({ message: 'error' }, 500);
        if (method === 'POST') {
          const body = await (req as Request).json();
          const created = {
            ...body,
            id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa4',
            project_id: sampleProject.id,
            created_at: '2026-07-18T00:00:00Z',
          };
          currentStatuses.push(created);
          return jsonResponse(created, 201);
        }
        return jsonResponse(currentStatuses);
      }
      if (pathname.endsWith('/statuses/reorder') && method === 'PUT') {
        const body = await (req as Request).json();
        currentStatuses = body.ids.map((id: string, position: number) => ({
          ...currentStatuses.find((status) => status.id === id)!,
          position,
        }));
        return jsonResponse(currentStatuses);
      }
      if (pathname.includes('/statuses/') && (method === 'PUT' || method === 'DELETE')) {
        const id = pathname.split('/').at(-1)!;
        if (method === 'DELETE') {
          currentStatuses = currentStatuses.filter((status) => status.id !== id);
          return new Response(null, { status: 204 });
        }
        const body = await (req as Request).json();
        if (body.is_default === true) {
          currentStatuses = currentStatuses.map((status) => ({ ...status, is_default: false }));
        }
        const index = currentStatuses.findIndex((status) => status.id === id);
        currentStatuses[index] = { ...currentStatuses[index]!, ...body };
        return jsonResponse(currentStatuses[index]);
      }
      if (method === 'PUT' || method === 'DELETE') {
        if (overrides.rejectWrite) {
          return jsonResponse({ message: 'error' }, overrides.rejectWrite);
        }
        if (method === 'DELETE') return new Response(null, { status: 204 });
        const body = await (req as Request).json();
        return jsonResponse({ ...sampleProject, ...body });
      }
      // プロジェクト一覧（useResolvedProjectId 用）
      if (url.includes('/projects')) {
        return jsonResponse(overrides.noProject ? [] : [sampleProject]);
      }
      return jsonResponse([]);
    });
    globalThis.fetch = fetchSpy;
    return () => {
      globalThis.fetch = original;
      fetchSpy = null;
    };
  };
}

function storyDecorator() {
  return () => ({
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
  });
}

const meta = {
  title: 'Pages/ProjectSettings',
  component: ProjectSettingsPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクト設定ページ（デザイン準拠のフルページ）。Details カード＋Danger zone。fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof ProjectSettingsPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: '設定表示（一般セクション・プリフィル＋キー変更不可）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'プロジェクト設定' }),
    ).resolves.toBeInTheDocument();
    // v2: 左ナビ + 一般セクションが既定表示
    await expect(
      canvas.findByRole('navigation', { name: '設定セクション' }),
    ).resolves.toBeInTheDocument();
    await expect(canvas.findByRole('heading', { name: '一般' })).resolves.toBeInTheDocument();
    await expect(canvas.findByLabelText('名前')).resolves.toHaveValue('Team Alpha');
    await expect(canvas.getByLabelText('キー')).toBeDisabled();
    await expect(canvas.getByLabelText('キー')).toHaveValue('ALPHA');
  },
};

export const SaveFlow: Story = {
  name: '編集→保存（PUT）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    const name = await canvas.findByLabelText('名前');
    await user.clear(name);
    await user.type(name, 'Renamed');
    await user.click(canvas.getByRole('button', { name: '変更を保存' }));

    const put = (fetchSpy!.mock.calls as [Request | string][])
      .map(([req]) => req)
      .filter((req): req is Request => typeof req !== 'string')
      .find((req) => req.method === 'PUT');
    await expect(put).toBeTruthy();
    await expect(put!.url).toContain(`/projects/${sampleProject.id}`);
  },
};

export const DeleteFlow: Story = {
  name: '削除セクション → 確認 → DELETE',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    // ナビで削除セクションへ
    const nav = await canvas.findByRole('navigation', { name: '設定セクション' });
    await user.click(within(nav).getByRole('button', { name: '削除' }));
    await expect(
      canvas.findByText('このプロジェクトとすべてのタスクを完全に削除します。'),
    ).resolves.toBeInTheDocument();
    await user.click(canvas.getByRole('button', { name: 'プロジェクトを削除' }));
    await expect(
      page.findByText('「Team Alpha」を削除します。この操作は取り消せません。'),
    ).resolves.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: '削除する' }));

    const del = (fetchSpy!.mock.calls as [Request | string][])
      .map(([req]) => req)
      .filter((req): req is Request => typeof req !== 'string')
      .find((req) => req.method === 'DELETE');
    await expect(del).toBeTruthy();
    await expect(del!.url).toContain(`/projects/${sampleProject.id}`);
  },
};

export const WorkflowListAndUuidResolution: Story = {
  name: 'ワークフロー一覧・UUID 解決',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: 'ワークフロー' }));
    await expect(canvas.findByLabelText('レビュー中の名前')).resolves.toHaveValue('レビュー中');
    await expect(
      canvas
        .getAllByRole('checkbox', { name: 'Default' })
        .filter((checkbox) => checkbox.getAttribute('aria-checked') === 'true'),
    ).toHaveLength(1);
    await expect(
      canvas
        .getAllByRole('checkbox', { name: 'Done state' })
        .filter((checkbox) => checkbox.getAttribute('aria-checked') === 'true'),
    ).toHaveLength(1);

    const statusesRequest = (fetchSpy!.mock.calls as [Request | string][])
      .map(([request]) => (typeof request === 'string' ? request : request.url))
      .find((url) => url.endsWith('/statuses'));
    await expect(statusesRequest).toContain(
      `/v1/tenants/${TENANT_UUID}/projects/${sampleProject.id}/statuses`,
    );
    await expect(statusesRequest).not.toContain(mockContext.routeParams.tenant);
    await expect(statusesRequest).not.toContain(mockContext.routeParams.projectKey);
  },
};

export const WorkflowAddEditAndReorder: Story = {
  name: '追加・名前色編集・並び替え',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: 'ワークフロー' }));
    await canvas.findByLabelText('レビュー中の名前');

    await user.type(canvas.getByLabelText('新しいステータス名'), 'Blocked');
    await user.click(canvas.getByRole('button', { name: '追加' }));
    await expect(canvas.findByLabelText('Blockedの名前')).resolves.toBeInTheDocument();

    const reviewName = canvas.getByLabelText('レビュー中の名前');
    await user.clear(reviewName);
    await user.type(reviewName, '確認待ち');
    await fireEvent.input(canvas.getByLabelText('レビュー中の色'), {
      target: { value: '#a855f7' },
    });
    await user.click(canvas.getByRole('button', { name: 'レビュー中を保存' }));
    await expect(canvas.findByLabelText('確認待ちの名前')).resolves.toHaveValue('確認待ち');

    await user.click(canvas.getByRole('button', { name: '確認待ちを上へ' }));
    const reorder = (fetchSpy!.mock.calls as [Request | string][])
      .map(([request]) => request)
      .filter((request): request is Request => typeof request !== 'string')
      .find((request) => request.method === 'PUT' && request.url.endsWith('/statuses/reorder'));
    await expect(reorder).toBeTruthy();
  },
};

export const WorkflowUniqueFlagsAndDelete: Story = {
  name: 'Default・Done 一意切替・削除確認',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: 'ワークフロー' }));
    await canvas.findByLabelText('レビュー中の名前');

    const getReviewRow = () => canvas.getByLabelText('レビュー中の名前').closest('li')!;
    await user.click(within(getReviewRow()).getByRole('checkbox', { name: 'Default' }));
    await waitFor(() =>
      expect(within(getReviewRow()).getByRole('checkbox', { name: 'Default' })).toBeChecked(),
    );
    await user.click(within(getReviewRow()).getByRole('checkbox', { name: 'Done state' }));
    await waitFor(() =>
      expect(within(getReviewRow()).getByRole('checkbox', { name: 'Done state' })).toBeChecked(),
    );

    await user.click(canvas.getByRole('button', { name: 'Todoを削除' }));
    await expect(
      page.findByRole('heading', { name: 'ステータスを削除しますか？' }),
    ).resolves.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: '削除する' }));
    await waitFor(() => expect(canvas.queryByLabelText('Todoの名前')).not.toBeInTheDocument());

    const writes = (fetchSpy!.mock.calls as [Request | string][])
      .map(([request]) => request)
      .filter((request): request is Request => typeof request !== 'string');
    await expect(
      writes.filter((request) => request.method === 'PUT').length,
    ).toBeGreaterThanOrEqual(3);
    await expect(writes.some((request) => request.method === 'DELETE')).toBe(true);
  },
};

export const WorkflowEmpty: Story = {
  name: 'ワークフロー空状態',
  beforeEach: mockFetch({ statuses: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'ワークフロー' }));
    await expect(
      canvas.findByText('ステータスがありません。最初のステータスを追加してください。'),
    ).resolves.toBeInTheDocument();
  },
};

export const WorkflowError: Story = {
  name: 'ワークフロー取得エラー',
  beforeEach: mockFetch({ rejectStatuses: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'ワークフロー' }));
    await expect(
      canvas.findByText('ステータスを読み込めませんでした'),
    ).resolves.toBeInTheDocument();
  },
};

export const WorkflowLoading: Story = {
  name: 'ワークフロー読み込み中',
  beforeEach: mockFetch({ hangStatuses: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'ワークフロー' }));
    await expect(canvas.findByText('ステータスを読み込み中…')).resolves.toBeInTheDocument();
  },
};

export const NotFound: Story = {
  name: 'プロジェクトなし',
  beforeEach: mockFetch({ noProject: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('プロジェクトが見つかりません')).resolves.toBeInTheDocument();
  },
};

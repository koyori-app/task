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

/** 見ている人のテナントでの関わり方。既定はオーナー（一般の保存と削除を出す） */
type Viewer = { membership: 'Owner' | 'Member'; member_role: 'Admin' | 'Member' | null };
const OWNER: Viewer = { membership: 'Owner', member_role: null };

const sampleTenants = (displayId: string, viewer: Viewer = OWNER) => [
  {
    id: TENANT_UUID,
    display_id: displayId,
    name: 'テストテナント',
    description: '',
    icon_url: '',
    owner_id: '00000000-0000-0000-0000-000000000002',
    require_2fa: false,
    ...viewer,
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
    is_default_done: false,
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
    is_default_done: false,
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
    is_default_done: true,
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
  viewer?: Viewer;
  rejectWrite?: number;
  rejectStatuses?: boolean;
  hangStatuses?: boolean;
  noProject?: boolean;
  statuses?: typeof sampleStatuses;
};

let fetchSpy: ReturnType<typeof fn> | null = null;
/** プロジェクトの PUT の body。mock 側が Request を読み切るので、送った中身はここに控える。 */
let putBodies: unknown[] = [];

function mockFetch(overrides: MockOptions = {}) {
  return () => {
    const original = globalThis.fetch;
    putBodies = [];
    let currentStatuses = structuredClone(overrides.statuses ?? sampleStatuses);
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      if (isListTenantsUrl(url)) {
        return jsonResponse(sampleTenants(mockContext.routeParams.tenant, overrides.viewer));
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
        putBodies.push(body);
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

/** テナントの Member。一般は読み取りだけで、保存と削除を出さない。他の節には入れる */
export const MemberReadOnly: Story = {
  name: 'テナントの Member（一般は読み取りだけ・削除なし）',
  beforeEach: mockFetch({ viewer: { membership: 'Member', member_role: 'Member' } }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(canvas.findByRole('heading', { name: '一般' })).resolves.toBeInTheDocument();
    await expect(
      canvas.findByText(
        '名前・説明・アイコンを変更できるのは、テナントのオーナーと Admin だけです。',
      ),
    ).resolves.toBeInTheDocument();
    await expect(canvas.queryByRole('button', { name: '変更を保存' })).toBeNull();
    await expect(canvas.queryByLabelText('名前')).toBeNull();
    const nav = await canvas.findByRole('navigation', { name: '設定セクション' });
    await expect(within(nav).queryByRole('button', { name: '削除' })).toBeNull();
    // ワークフローなど、API が入れる人に許している節へは入れる
    await user.click(within(nav).getByRole('button', { name: 'ワークフロー' }));
    await expect(canvas.findByLabelText('レビュー中の名前')).resolves.toHaveValue('レビュー中');
  },
};

/** 保存が 403 になったとき、権限不足と分かる文言を出す */
export const SaveForbidden: Story = {
  name: '保存が 403（権限不足の文言）',
  beforeEach: mockFetch({ rejectWrite: 403 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await canvas.findByLabelText('名前');
    await user.click(canvas.getByRole('button', { name: '変更を保存' }));
    await expect(
      canvas.findByText(
        'プロジェクトの設定を変更できるのは、テナントのオーナーと Admin だけです。',
      ),
    ).resolves.toBeInTheDocument();
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

export const IconSearchAndSave: Story = {
  name: '絵文字を検索して選ぶ → PUT に icon_emoji が載る',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();

    await user.click(await canvas.findByRole('button', { name: /^アイコンを選択/ }));
    const panel = within(await canvas.findByRole('dialog', { name: 'アイコンを選択' }));
    const search = panel.getByRole('searchbox', { name: '絵文字を検索' });
    await user.type(search, 'ろけっと');
    // 名前はデータの読み込み後に付く。おすすめと検索結果の両方に出ることがある
    await waitFor(() =>
      expect(panel.getAllByRole('button', { name: 'ロケット' }).length).toBeGreaterThan(0),
    );
    await user.type(search, '{Enter}');
    await expect(
      canvas.findByRole('button', { name: 'アイコンを選択: ロケット' }),
    ).resolves.toBeInTheDocument();

    await user.click(canvas.getByRole('button', { name: '変更を保存' }));
    await waitFor(() => expect(putBodies.at(-1)).toMatchObject({ icon_emoji: '🚀' }));
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
    await expect(canvas.getAllByRole('checkbox', { name: 'Default', checked: true })).toHaveLength(
      1,
    );
    await expect(
      canvas.getAllByRole('checkbox', { name: 'Done state', checked: true }),
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
    await expect(writes.filter((request) => request.method === 'PUT')).toHaveLength(2);
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

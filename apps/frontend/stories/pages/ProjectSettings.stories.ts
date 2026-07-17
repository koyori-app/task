import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
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

type MockOptions = { rejectWrite?: number; noProject?: boolean };

let fetchSpy: ReturnType<typeof fn> | null = null;

function mockFetch(overrides: MockOptions = {}) {
  return () => {
    const original = globalThis.fetch;
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      if (isListTenantsUrl(url)) {
        return jsonResponse(sampleTenants(mockContext.routeParams.tenant));
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

export const NotFound: Story = {
  name: 'プロジェクトなし',
  beforeEach: mockFetch({ noProject: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('プロジェクトが見つかりません')).resolves.toBeInTheDocument();
  },
};

import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import ProjectNewPage from '@/pages/@tenant/projects/new/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/tenant-123/projects/new',
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

type MockOptions = { rejectCreate?: number };

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
      if (method === 'POST' && url.includes('/projects')) {
        if (overrides.rejectCreate) {
          return jsonResponse({ message: 'error' }, overrides.rejectCreate);
        }
        const body = await (req as Request).json();
        return jsonResponse(
          {
            id: 'proj-new',
            tenant_id: TENANT_UUID,
            name: body.name,
            description: body.description ?? '',
            key: body.key ?? 'AUTO',
            is_personal: false,
            icon_emoji: body.icon_emoji ?? null,
            icon_url: null,
            personal_owner_id: null,
          },
          201,
        );
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
  title: 'Pages/ProjectNew',
  component: ProjectNewPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクト作成ページ（デザイン準拠のフルページ）。Details カード＋フッター。fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof ProjectNewPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: '作成フォーム表示',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'プロジェクトを作成' }),
    ).resolves.toBeInTheDocument();
    await expect(canvas.findByLabelText('名前')).resolves.toBeInTheDocument();
    await expect(canvas.findByLabelText('キー')).resolves.toBeInTheDocument();
    // v2: 既定ステータスのプレビューカード
    await expect(canvas.findByText('ワークフローステータス')).resolves.toBeInTheDocument();
    for (const status of ['Backlog', 'Todo', 'In Progress', 'Done']) {
      await expect(canvas.findByText(status)).resolves.toBeInTheDocument();
    }
    await expect(canvas.findByText('Default')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('Done state')).resolves.toBeInTheDocument();
  },
};

export const CreateFlow: Story = {
  name: '作成（キー自動提案→POST）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    const name = await canvas.findByLabelText('名前');
    await user.type(name, 'New Project');
    await expect(canvas.getByLabelText('キー')).toHaveValue('NEWPROJECT');

    await user.click(canvas.getByRole('button', { name: 'プロジェクトを作成' }));

    const post = (fetchSpy!.mock.calls as [Request | string][])
      .map(([req]) => req)
      .filter((req): req is Request => typeof req !== 'string')
      .find((req) => req.method === 'POST');
    await expect(post).toBeTruthy();
    await expect(post!.url).toContain(`/v1/tenants/${TENANT_UUID}/projects`);
  },
};

export const CreateFailure: Story = {
  name: '作成失敗',
  beforeEach: mockFetch({ rejectCreate: 500 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    const name = await canvas.findByLabelText('名前');
    await user.type(name, 'X Project');
    await user.click(canvas.getByRole('button', { name: 'プロジェクトを作成' }));
    await expect(await canvas.findByRole('alert')).toHaveTextContent(
      'プロジェクトを作成できませんでした',
    );
  },
};

import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, within } from 'storybook/test';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import { provide } from 'vue';

import LabelsPage from '@/pages/@tenant/projects/@projectKey/labels/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';
const TENANT_DISPLAY_ID = 'tenant-123';
const TENANT_UUID = '11111111-1111-4111-8111-111111111111';
const PROJECT_UUID = '22222222-2222-4222-8222-222222222222';

const mockContext = {
  urlPathname: `/${TENANT_DISPLAY_ID}/projects/ENG/labels`,
  routeParams: { tenant: TENANT_DISPLAY_ID, projectKey: 'ENG' },
};

const sampleTenant = {
  id: TENANT_UUID,
  display_id: TENANT_DISPLAY_ID,
  name: 'テストテナント',
  description: '',
  icon_url: '',
  owner_id: '00000000-0000-4000-8000-000000000002',
  require_2fa: false,
};

const sampleProject = {
  id: PROJECT_UUID,
  key: 'ENG',
  name: 'エンジニアリング',
  description: '',
  tenant_id: TENANT_UUID,
  is_personal: false,
};

const sampleLabels = [
  {
    id: '33333333-3333-4333-8333-333333333333',
    name: 'bug',
    description: '不具合',
    color: '#ef4444',
    icon_url: null,
    project_id: PROJECT_UUID,
  },
];

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

type MockOptions = {
  tenants?: (typeof sampleTenant)[];
  projects?: (typeof sampleProject)[];
  rejectLabels?: boolean;
  hang?: boolean;
};

function isListTenantsPath(pathname: string) {
  return /\/v1\/tenants\/?$/.test(pathname);
}

function createMockFetch(options: MockOptions = {}) {
  const original = globalThis.fetch;
  const fetchSpy = fn().mockImplementation(async (request: Request | string) => {
    const url = typeof request === 'string' ? request : request.url;
    const pathname = new URL(url, 'http://localhost').pathname;

    if (options.hang) return new Promise<Response>(() => {});
    if (isListTenantsPath(pathname)) return jsonResponse(options.tenants ?? [sampleTenant]);
    if (pathname.endsWith('/labels')) {
      if (options.rejectLabels) return jsonResponse({ message: 'server error' }, 500);
      return jsonResponse(sampleLabels);
    }
    if (pathname.endsWith('/projects')) return jsonResponse(options.projects ?? [sampleProject]);
    return jsonResponse({ message: 'not found' }, 404);
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

function mockFetch(options: MockOptions = {}) {
  return () => {
    activeMock = createMockFetch(options);
    return () => {
      activeMock?.restore();
      activeMock = null;
    };
  };
}

function storyDecorator() {
  return () => ({
    setup() {
      provide(
        VUE_QUERY_CLIENT,
        new QueryClient({
          defaultOptions: {
            queries: { retry: false, gcTime: 0, staleTime: 0 },
            mutations: { retry: false },
          },
        }),
      );
      provide(PAGE_CONTEXT_KEY, mockContext);
    },
    template: '<story />',
  });
}

const meta = {
  title: 'Pages/Labels',
  component: LabelsPage,
  tags: ['autodocs'],
  parameters: { layout: 'padded' },
  decorators: [storyDecorator()],
} satisfies Meta<typeof LabelsPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('bug')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('不具合')).resolves.toBeInTheDocument();
  },
};

export const ResolvesTenantUuid: Story = {
  name: 'テナント解決（display_id → UUID）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('bug')).resolves.toBeInTheDocument();

    const calledUrls = (activeMock!.fetchSpy.mock.calls as [Request | string][]).map(([request]) =>
      typeof request === 'string' ? request : request.url,
    );
    const tenantScopedUrls = calledUrls.filter(
      (url) => url.includes('/projects') || url.includes('/labels'),
    );
    await expect(tenantScopedUrls.length).toBe(2);
    for (const url of tenantScopedUrls) {
      await expect(url).toContain(`/v1/tenants/${TENANT_UUID}/projects`);
      await expect(url).not.toContain(`/v1/tenants/${TENANT_DISPLAY_ID}/`);
    }
  },
};

export const TenantNotFound: Story = {
  beforeEach: mockFetch({ tenants: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('Tenant not found')).resolves.toBeInTheDocument();
  },
};

export const ProjectNotFound: Story = {
  beforeEach: mockFetch({ projects: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('Project not found')).resolves.toBeInTheDocument();
  },
};

export const ApiError: Story = {
  beforeEach: mockFetch({ rejectLabels: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('Failed to fetch labels')).resolves.toBeInTheDocument();
  },
};

export const Loading: Story = {
  beforeEach: mockFetch({ hang: true }),
};

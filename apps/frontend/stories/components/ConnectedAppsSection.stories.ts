import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import ConnectedAppsSection from '@/components/settings/ConnectedAppsSection.vue';

const TENANT_ID = '11111111-1111-1111-1111-111111111111';
const SECOND_TENANT_ID = '44444444-4444-4444-4444-444444444444';
const CLAUDE_GRANT_ID = '00000000-0000-4000-8000-000000000031';
const CODEX_GRANT_ID = '00000000-0000-4000-8000-000000000032';

const sampleGrants = [
  {
    id: CLAUDE_GRANT_ID,
    client_id: 'https://claude.ai/oauth/mcp-oauth-client-metadata',
    client_name: 'Claude',
    tenant_ids: [TENANT_ID],
    scopes: [
      'read:tenant',
      'read:project',
      'read:task',
      'write:task',
      'read:milestone',
      'read:sprint',
    ],
    expires_at: '2099-11-14T00:00:00Z',
    last_used_at: '2026-01-01T00:00:00Z',
  },
  {
    id: CODEX_GRANT_ID,
    client_id: 'https://chatgpt.com/oauth/codex/client.json',
    client_name: 'Codex',
    tenant_ids: [TENANT_ID, SECOND_TENANT_ID],
    scopes: ['read:task'],
    expires_at: '2099-11-14T00:00:00Z',
    last_used_at: null,
  },
];

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

let fetchSpy: ReturnType<typeof fn> | null = null;

/** 連携中のアプリの API をインメモリの配列で応答する fetch モック */
function mockFetch(overrides: { empty?: boolean; listFails?: boolean } = {}) {
  return () => {
    const original = globalThis.fetch;
    let grants = overrides.empty ? [] : sampleGrants.map((grant) => ({ ...grant }));
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      const itemMatch = pathname.match(/\/v1\/oauth_grants\/([^/]+)$/);

      if (method === 'GET' && pathname.endsWith('/v1/tenants')) {
        return jsonResponse([
          { id: TENANT_ID, display_id: 'acme', name: 'Acme Inc', membership: 'Owner' },
          { id: SECOND_TENANT_ID, display_id: 'beta', name: 'Beta LLC', membership: 'Member' },
        ]);
      }
      if (method === 'GET' && pathname.endsWith('/v1/oauth_grants')) {
        return overrides.listFails
          ? jsonResponse({ message: 'internal-error' }, 500)
          : jsonResponse(grants);
      }
      if (method === 'DELETE' && itemMatch) {
        grants = grants.filter((grant) => grant.id !== itemMatch[1]);
        return new Response(null, { status: 204 });
      }
      return jsonResponse({ message: 'not-found' }, 404);
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
    },
    template: '<story />',
  });
}

const meta = {
  title: 'Components/Settings/ConnectedAppsSection',
  component: ConnectedAppsSection,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'アカウント設定の「連携中のアプリ」（`/settings/apps`）。MCP の OAuth で接続を許可したアプリの一覧（名前・client_id の host・テナント・スコープ数・期限・最終使用）と、確認のダイアログを経た接続の解除。fetch モックで検証。',
      },
    },
  },
  decorators: [storyDecorator()],
} satisfies Meta<typeof ConnectedAppsSection>;

export default meta;
type Story = StoryObj<typeof meta>;

const requestsOf = (method: string) =>
  (fetchSpy!.mock.calls as [Request | string][])
    .map(([req]) => req)
    .filter((req): req is Request => typeof req !== 'string')
    .filter((req) => req.method === method);

export const Default: Story = {
  name: '一覧表示（名前＋host＋テナント＋スコープ数＋期限＋最終使用）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: '連携中のアプリ' }),
    ).resolves.toBeInTheDocument();
    await expect(canvas.findByText('Claude')).resolves.toBeInTheDocument();
    await expect(canvas.getByText('claude.ai')).toBeInTheDocument();
    await expect(canvas.getByText('6 スコープ')).toBeInTheDocument();
    await expect(canvas.findByText('Codex')).resolves.toBeInTheDocument();
    await expect(canvas.getByText('chatgpt.com')).toBeInTheDocument();
    await expect(canvas.getByText('未使用')).toBeInTheDocument();
    // テナント名は一覧の取得より遅れて解決されるので、現れるまで待つ
    await expect(canvas.findByText('Acme Inc、Beta LLC')).resolves.toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: '空状態',
  beforeEach: mockFetch({ empty: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('連携中のアプリはありません。')).resolves.toBeInTheDocument();
  },
};

export const LoadError: Story = {
  name: '一覧の取得の失敗',
  beforeEach: mockFetch({ listFails: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByText('連携中のアプリの一覧を取得できませんでした。再読み込みしてください。'),
    ).resolves.toBeInTheDocument();
  },
};

export const DisconnectFlow: Story = {
  name: '接続の解除（確認ダイアログ → DELETE）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();
    await canvas.findByText('Codex');

    const buttons = canvas.getAllByRole('button', { name: '接続を解除' });
    await user.click(buttons[1]);
    await expect(page.findByText('接続を解除しますか？')).resolves.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: '解除する' }));

    await waitFor(() => expect(canvas.queryByText('Codex')).not.toBeInTheDocument());
    const [del] = requestsOf('DELETE');
    await expect(del).toBeTruthy();
    await expect(del.url).toContain(`/v1/oauth_grants/${CODEX_GRANT_ID}`);
  },
};

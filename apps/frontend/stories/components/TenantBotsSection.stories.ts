import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import { provide } from 'vue';
import { expect, userEvent, within } from 'storybook/test';
import TenantBotsSection from '@/components/tenants/TenantBotsSection.vue';

const TENANT = '11111111-1111-4111-8111-111111111111';
const BOT = '22222222-2222-4222-8222-222222222222';
const response = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });
function mockApi() {
  const original = globalThis.fetch;
  let suspended = false;
  let tokens = [
    {
      id: 'token-1',
      name: 'Release pipeline',
      token_last_four: '9d4e',
      scopes: ['read:task', 'write:task'],
      expires_at: '2099-12-01T00:00:00Z',
      last_used_at: null,
    },
  ];
  globalThis.fetch = async (request: Request | string | URL) => {
    const req = request as Request;
    const path = new URL(req.url, 'http://localhost').pathname;
    if (path.endsWith('/bots') && req.method === 'GET')
      return response([
        {
          id: BOT,
          tenant_id: TENANT,
          username: 'Release BOT',
          account_kind: 'bot',
          role: 'Member',
          is_suspended: suspended,
        },
      ]);
    if (path.endsWith('/projects'))
      return response([{ id: 'project-1', name: 'Release', key: 'REL' }]);
    if (path.endsWith('/members')) return response([]);
    if (path.endsWith(`/bots/${BOT}`) && req.method === 'PATCH') {
      suspended = (await req.json()).is_suspended;
      return new Response(null, { status: 204 });
    }
    if (path.endsWith('/tokens') && req.method === 'GET') return response(tokens);
    if (path.endsWith('/tokens') && req.method === 'POST')
      return response({ token: 'pat_story-only-example-secret', id: 'token-2' }, 201);
    if (req.method === 'DELETE') {
      tokens = [];
      return new Response(null, { status: 204 });
    }
    if (path.endsWith('/bots') && req.method === 'POST') return response({ id: BOT }, 201);
    return response({ message: 'not found' }, 404);
  };
  return () => {
    globalThis.fetch = original;
  };
}
const meta = {
  title: 'Components/Settings/TenantBotsSection',
  component: TenantBotsSection,
  args: { tenantId: TENANT },
  parameters: { layout: 'padded' },
  beforeEach: mockApi,
  decorators: [
    () => ({
      setup() {
        provide(
          VUE_QUERY_CLIENT,
          new QueryClient({
            defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } },
          }),
        );
      },
      template: '<div class="mx-auto max-w-3xl"><story /></div>',
    }),
  ],
} satisfies Meta<typeof TenantBotsSection>;
export default meta;
type Story = StoryObj<typeof meta>;
export const ManageTokens: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'PAT を管理' }));
    await expect(await canvas.findByRole('heading', { name: 'Release BOT の PAT' })).toBeVisible();
    await expect(await canvas.findByText('Release pipeline')).toBeVisible();
  },
};

export const IssueToken: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'PAT を管理' }));
    await userEvent.type(await canvas.findByLabelText('トークン名'), 'nightly-sync');
    await userEvent.click(canvas.getByRole('button', { name: 'PAT を発行' }));
    await expect(await canvas.findByTestId('created-bot-token')).toHaveTextContent(
      'pat_story-only-example-secret',
    );
  },
};

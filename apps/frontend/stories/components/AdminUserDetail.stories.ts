import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import AdminUserDetail from '@/components/admin/AdminUserDetail.vue';

const ME = '00000000-0000-4000-8000-00000000000a';
const TARGET = '00000000-0000-4000-8000-00000000000b';

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });

const user = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  username: id === ME ? 'root' : 'alice',
  email: id === ME ? 'root@example.com' : 'alice@example.com',
  email_verified: true,
  is_admin: id === ME,
  is_suspended: false,
  totp_enabled: id !== ME,
  has_password: true,
  ...over,
});

function mockFetch() {
  const original = globalThis.fetch;
  globalThis.fetch = fn().mockImplementation(async (req: Request) => {
    const { pathname } = new URL(req.url, 'http://localhost');
    if (pathname.endsWith('/v1/auth/me')) return json(user(ME));
    if (pathname.endsWith('/v1/admin/users')) return json([user(ME), user(TARGET)]);
    if (pathname.endsWith('/auth-methods')) {
      return json({
        passkeys: [
          {
            id: '00000000-0000-4000-8000-0000000000c1',
            name: 'YubiKey',
            created_at: '2026-09-01T00:00:00Z',
          },
        ],
        oauth: [
          { provider: 'github', provider_login: 'octocat', created_at: '2026-09-01T00:00:00Z' },
        ],
      });
    }
    return new Response(null, { status: 204 });
  });
  return () => {
    globalThis.fetch = original;
  };
}

const meta = {
  title: 'Components/Admin/AdminUserDetail',
  component: AdminUserDetail,
  tags: ['autodocs'],
  args: { userId: TARGET },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          '管理者向けのユーザー詳細。権限・停止・パスワードリセット・2FA リセット・パスキー削除・OAuth 解除を、対象ユーザーと影響を明記した確認ダイアログ越しに行う。fetch モックで検証。',
      },
    },
  },
  decorators: [
    () => ({
      setup() {
        provide(
          VUE_QUERY_CLIENT,
          new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } }),
        );
      },
      template: '<story />',
    }),
  ],
  beforeEach: mockFetch,
} satisfies Meta<typeof AdminUserDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: '詳細と認証方法',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('YubiKey')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('octocat', { exact: false })).resolves.toBeInTheDocument();
  },
};

export const ConfirmReset2fa: Story = {
  name: '2FA リセットの確認ダイアログ',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: '2FA をリセット' }));
    await expect(
      within(document.body).findByText(/alice（alice@example.com）/),
    ).resolves.toBeInTheDocument();
  },
};

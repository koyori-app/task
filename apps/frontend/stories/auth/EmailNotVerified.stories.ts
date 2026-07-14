import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import EmailNotVerified from '@/components/auth/EmailNotVerified.vue';

const meta = {
  title: 'Auth/EmailNotVerified',
  component: EmailNotVerified,
  tags: ['autodocs'],
  parameters: {
    docs: {
      description: {
        component:
          'ログイン済みだが email_verified が false のユーザーに表示する案内画面。確認メールの再送（POST /v1/auth/resend-verification-email）とサインアウトを提供します。',
      },
    },
  },
  args: {
    email: 'user@example.com',
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
      },
      template: '<story />',
    }),
  ],
} satisfies Meta<typeof EmailNotVerified>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const RegistrationCompleted: Story = {
  args: {
    backHref: '/signin',
    resetHref: '/auth/reset-password',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('link', { name: 'サインインページへ戻る' })).toHaveAttribute(
      'href',
      '/signin',
    );
    await expect(canvas.getByRole('link', { name: 'パスワードを再設定する' })).toHaveAttribute(
      'href',
      '/auth/reset-password',
    );
  },
};

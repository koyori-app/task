import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import ResetPasswordPage from '@/pages/auth/reset-password/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const getRequestInfo = (input: RequestInfo | URL, init?: RequestInit) => {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase();
  return { url, method };
};

const makeContext = (token?: string) => ({
  urlPathname: '/auth/reset-password',
  urlParsed: { search: token ? { token } : {} },
  routeParams: {},
});

const withPageContext = (token?: string) => [
  () => ({
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: {
          queries: { retry: false, gcTime: 0, staleTime: 0 },
          mutations: { retry: false },
        },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
      provide(PAGE_CONTEXT_KEY, makeContext(token));
    },
    template: '<story />',
  }),
];

const meta = {
  title: 'Pages/ResetPassword',
  component: ResetPasswordPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
    argos: { fitToContent: false },
    docs: {
      description: {
        component:
          'パスワード再設定ページ。token クエリなしはリクエストフォーム、ありは新パスワード設定フォームを表示する。',
      },
    },
  },
} satisfies Meta<typeof ResetPasswordPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const RequestForm: Story = {
  name: 'リクエストフォーム（tokenなし）',
  decorators: withPageContext(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'パスワード再設定' }),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByRole('button', { name: '再設定リンクを送信' }),
    ).resolves.toBeInTheDocument();
  },
};

export const RequestSuccess: Story = {
  name: 'リクエスト成功（常に同一表示）',
  decorators: withPageContext(),
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation((input, init) => {
      const { url, method } = getRequestInfo(input, init);
      if (method === 'POST' && url.includes('/v1/auth/password-reset/request')) {
        return Promise.resolve(
          new Response(JSON.stringify({ message: 'sent' }), {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          }),
        );
      }
      return Promise.resolve(
        new Response(JSON.stringify({ message: 'not found' }), { status: 404 }),
      );
    });
    return () => {
      globalThis.fetch = original;
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const email = canvas.getByLabelText('メールアドレス');
    await userEvent.clear(email);
    await userEvent.type(email, 'test@example.com');
    await userEvent.tab();
    await userEvent.click(canvas.getByRole('button', { name: '再設定リンクを送信' }));
    await expect(
      canvas.findByRole('heading', { name: 'メールを送信しました' }),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByText(/が登録済みの場合、パスワード再設定用のリンクを送信しました/),
    ).resolves.toBeInTheDocument();
  },
};

export const CompleteForm: Story = {
  name: '新パスワード設定（token有効）',
  decorators: withPageContext('valid-token'),
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation((input, init) => {
      const { url, method } = getRequestInfo(input, init);
      if (method === 'POST' && url.includes('/v1/auth/password-reset/verify')) {
        return Promise.resolve(new Response(null, { status: 200 }));
      }
      if (method === 'POST' && url.includes('/v1/auth/password-reset/complete')) {
        return Promise.resolve(
          new Response(JSON.stringify({ message: 'ok' }), {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          }),
        );
      }
      return Promise.resolve(
        new Response(JSON.stringify({ message: 'not found' }), { status: 404 }),
      );
    });
    return () => {
      globalThis.fetch = original;
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: '新しいパスワードを設定' }),
    ).resolves.toBeInTheDocument();
    const password = canvas.getByLabelText('新しいパスワード');
    await userEvent.click(password);
    await userEvent.type(password, 'NewPassword123!');
    await userEvent.tab();
    await userEvent.click(canvas.getByRole('button', { name: 'パスワードを再設定する' }));
    await expect(
      canvas.findByRole('heading', { name: 'パスワードを再設定しました' }),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByRole('link', { name: 'サインインページへ' }),
    ).resolves.toBeInTheDocument();
  },
};

export const InvalidToken: Story = {
  name: 'リンク無効（token失効）',
  decorators: withPageContext('expired-token'),
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation((input, init) => {
      const { url, method } = getRequestInfo(input, init);
      if (method === 'POST' && url.includes('/v1/auth/password-reset/verify')) {
        return Promise.resolve(
          new Response(JSON.stringify({ message: 'password-reset-token-not-found' }), {
            status: 404,
            headers: { 'Content-Type': 'application/json' },
          }),
        );
      }
      return Promise.resolve(
        new Response(JSON.stringify({ message: 'not found' }), { status: 404 }),
      );
    });
    return () => {
      globalThis.fetch = original;
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'リンクが無効です' }),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByRole('link', { name: '再設定リンクを再取得する' }),
    ).resolves.toBeInTheDocument();
  },
};

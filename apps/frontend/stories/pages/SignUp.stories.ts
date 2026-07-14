import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import SignUpPage from '@/pages/signup/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/signup',
  routeParams: {},
};

const getRequestInfo = (input: RequestInfo | URL, init?: RequestInit) => {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase();
  return { url, method };
};

async function fillSignUpForm(canvas: ReturnType<typeof within>) {
  const username = canvas.getByLabelText('ユーザー名');
  await userEvent.clear(username);
  await userEvent.type(username, 'testuser');
  await userEvent.tab();

  const email = canvas.getByLabelText('メールアドレス');
  await userEvent.clear(email);
  await userEvent.type(email, 'test@example.com');
  await userEvent.tab();

  const password = canvas.getByLabelText('パスワード');
  await userEvent.click(password);
  await userEvent.clear(password);
  await userEvent.type(password, 'password123');
  await userEvent.tab();

  await expect(canvas.getByRole('button', { name: 'アカウント作成' })).toBeEnabled();
}

function mockSuccessfulRegistration(status: 200 | 201) {
  const original = globalThis.fetch;
  globalThis.fetch = fn().mockImplementation((input, init) => {
    const { url, method } = getRequestInfo(input, init);
    if (method === 'POST' && url.includes('/v1/auth/register')) {
      return Promise.resolve(new Response(null, { status }));
    }
    return Promise.resolve(new Response(JSON.stringify({ message: 'not found' }), { status: 404 }));
  });
  return () => {
    globalThis.fetch = original;
  };
}

async function assertRegistrationCompleted(canvasElement: HTMLElement) {
  const canvas = within(canvasElement);
  await fillSignUpForm(canvas);
  await userEvent.click(canvas.getByRole('button', { name: 'アカウント作成' }));
  await expect(
    canvas.findByRole('heading', { name: 'メールアドレスを確認してください' }),
  ).resolves.toBeInTheDocument();
  await expect(canvas.getByRole('link', { name: 'サインインページへ戻る' })).toHaveAttribute(
    'href',
    '/signin',
  );
  await expect(canvas.getByRole('link', { name: 'パスワードを再設定する' })).toHaveAttribute(
    'href',
    '/auth/reset-password',
  );
}

const meta = {
  title: 'Pages/SignUp',
  component: SignUpPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
    docs: {
      description: {
        component: 'サインアップページ。fetch モックで apiClient を差し替え済み。',
      },
    },
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
        provide(PAGE_CONTEXT_KEY, mockContext);
      },
      template: '<story />',
    }),
  ],
} satisfies Meta<typeof SignUpPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: 'デフォルト',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.findByRole('heading', { name: 'アカウント作成' }),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByRole('button', { name: 'アカウント作成' }),
    ).resolves.toBeInTheDocument();
  },
};

export const RegisterError: Story = {
  // #26: バックエンドは列挙対策で既存メールでも 201 を返すため 409 は発生しない。
  // 登録失敗の代表としてサーバーエラー（500）をモックする。
  name: '登録エラー（500）',
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation((input, init) => {
      const { url, method } = getRequestInfo(input, init);
      if (method === 'POST' && url.includes('/v1/auth/register')) {
        return Promise.resolve(
          new Response(JSON.stringify({ message: 'internal-server-error' }), {
            status: 500,
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
    await fillSignUpForm(canvas);
    await userEvent.click(canvas.getByRole('button', { name: 'アカウント作成' }));
    await expect(
      canvas.findByText('登録に失敗しました。時間をおいて再度お試しください。'),
    ).resolves.toBeInTheDocument();
  },
};

export const RegisterSuccess: Story = {
  name: '登録成功（200）',
  beforeEach() {
    return mockSuccessfulRegistration(200);
  },
  play: async ({ canvasElement }) => assertRegistrationCompleted(canvasElement),
};

export const RegisterSuccess201: Story = {
  name: '登録成功（201）',
  beforeEach() {
    return mockSuccessfulRegistration(201);
  },
  play: async ({ canvasElement }) => assertRegistrationCompleted(canvasElement),
};

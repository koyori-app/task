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
  // #135: 列挙対策により API は新規/既存を区別せず 201 を返すため、
  // 成功表示は常に同一で、既存アカウント向けの導線（サインイン/再設定）を含む。
  name: '登録成功（201・成功表示と導線）',
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation((input, init) => {
      const { url, method } = getRequestInfo(input, init);
      if (method === 'POST' && url.includes('/v1/auth/register')) {
        return Promise.resolve(
          new Response(JSON.stringify('Register successful'), {
            status: 201,
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
      canvas.findByRole('heading', { name: 'メールを送信しました' }),
    ).resolves.toBeInTheDocument();
    // 既存メールかどうかを推測できる文言（「確認メール」等）を表示しない
    expect(canvas.queryByText(/確認メール/)).not.toBeInTheDocument();
    await expect(canvas.findByRole('link', { name: 'サインインへ' })).resolves.toBeInTheDocument();
    await expect(
      canvas.findByRole('link', { name: 'パスワードを再設定する' }),
    ).resolves.toBeInTheDocument();
  },
};

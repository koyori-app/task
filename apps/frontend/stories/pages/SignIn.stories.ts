import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import SignInPage from '@/pages/signin/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/signin',
  routeParams: {},
};

let locationAssignSpy: ReturnType<typeof fn>;
let originalLocationAssign: Location['assign'];

const getRequestInfo = (input: RequestInfo | URL, init?: RequestInit) => {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase();
  return { url, method };
};

const stubLocationAssign = () => {
  locationAssignSpy = fn();
  originalLocationAssign = Location.prototype.assign;
  Location.prototype.assign = function (url: string | URL) {
    locationAssignSpy(url);
  };
};

const restoreLocationAssign = () => {
  Location.prototype.assign = originalLocationAssign;
};

const meResponse = () =>
  new Response(
    JSON.stringify({
      id: '00000000-0000-0000-0000-000000000001',
      email: 'test@example.com',
      username: 'testuser',
      email_verified: true,
      is_admin: false,
      is_suspended: false,
      totp_enabled: false,
    }),
    { status: 200, headers: { 'Content-Type': 'application/json' } },
  );

async function fillSignInForm(canvas: ReturnType<typeof within>) {
  const email = canvas.getByLabelText('メールアドレス');
  await userEvent.clear(email);
  await userEvent.type(email, 'test@example.com');
  await userEvent.tab();

  const password = canvas.getByLabelText('パスワード');
  await userEvent.click(password);
  await userEvent.clear(password);
  await userEvent.type(password, 'password123');
  await userEvent.tab();

  await expect(canvas.getByRole('button', { name: 'サインイン' })).toBeEnabled();
}

const meta = {
  title: 'Pages/SignIn',
  component: SignInPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
    docs: {
      description: {
        component: 'サインインページ。fetch モックで apiClient を差し替え済み。',
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
} satisfies Meta<typeof SignInPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: 'デフォルト',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('おかえりなさい')).resolves.toBeInTheDocument();
    await expect(canvas.findByRole('button', { name: 'サインイン' })).resolves.toBeInTheDocument();
  },
};

export const LoginError: Story = {
  name: 'ログインエラー（401）',
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation((input, init) => {
      const { url, method } = getRequestInfo(input, init);
      if (method === 'POST' && url.includes('/v1/auth/login')) {
        return Promise.resolve(
          new Response(JSON.stringify({ message: 'Unauthorized' }), {
            status: 401,
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
    await fillSignInForm(canvas);
    await userEvent.click(canvas.getByRole('button', { name: 'サインイン' }));
    await expect(
      canvas.findByText('メールアドレスまたはパスワードが正しくありません。'),
    ).resolves.toBeInTheDocument();
  },
};

export const LoginSuccess: Story = {
  name: 'ログイン成功（204）',
  decorators: [
    () => ({
      setup() {
        const queryClient = new QueryClient({
          defaultOptions: {
            queries: { retry: false, gcTime: 0, staleTime: 0 },
            mutations: { retry: false },
          },
        });
        queryClient.invalidateQueries = () => new Promise(() => {});
        provide(VUE_QUERY_CLIENT, queryClient);
        provide(PAGE_CONTEXT_KEY, mockContext);
      },
      template: '<story />',
    }),
  ],
  beforeEach() {
    stubLocationAssign();
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation((input, init) => {
      const { url, method } = getRequestInfo(input, init);
      if (method === 'POST' && url.includes('/v1/auth/login')) {
        locationAssignSpy('/');
        return Promise.resolve(new Response(null, { status: 204 }));
      }
      if (method === 'GET' && url.includes('/v1/auth/me')) {
        return Promise.resolve(meResponse());
      }
      return Promise.resolve(
        new Response(JSON.stringify({ message: 'not found' }), { status: 404 }),
      );
    });
    return () => {
      globalThis.fetch = original;
      restoreLocationAssign();
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await fillSignInForm(canvas);
    await userEvent.click(canvas.getByRole('button', { name: 'サインイン' }));
    await expect(locationAssignSpy).toHaveBeenCalledWith('/');
  },
};

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defineComponent } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import { createTestApiClient } from '@/lib/api-vue-query';
import { useAuthSession } from '../useAuthSession';
import { useAuthStore } from '@/stores/auth';

const mockUser = {
  id: '00000000-0000-0000-0000-000000000001',
  email: 'test@example.com',
  username: 'testuser',
  email_verified: true,
  is_admin: false,
  is_suspended: false,
  totp_enabled: false,
};

const { mockPagePathname } = vi.hoisted(() => ({
  mockPagePathname: { value: '/' as string },
}));

vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({ urlPathname: mockPagePathname.value }),
}));

const fetchMock = vi.fn(async (input: Request) => {
  const url = input.url;
  const method = input.method.toUpperCase();

  if (method === 'GET' && url.includes('/v1/auth/me')) {
    return new Response(JSON.stringify(mockUser), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  if (method === 'POST' && url.includes('/v1/auth/logout')) {
    return new Response(null, { status: 204 });
  }

  return new Response(JSON.stringify({ message: 'not found' }), {
    status: 404,
    headers: { 'Content-Type': 'application/json' },
  });
});

const testApi = createTestApiClient(fetchMock);

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    useMeQuery: (options?: { enabled?: import('vue').MaybeRefOrGetter<boolean> }) =>
      testApi.useQuery('get', '/v1/auth/me', undefined, {
        retry: false,
        ...(options?.enabled !== undefined ? { enabled: options.enabled } : {}),
      }),
    useLogoutMutation: () => testApi.useMutation('post', '/v1/auth/logout'),
    meQueryOptions: () => testApi.queryOptions('get', '/v1/auth/me', undefined, { retry: false }),
  };
});

describe('useAuthSession', () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    setActivePinia(createPinia());
    mockPagePathname.value = '/';
    queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    fetchMock.mockClear();
  });

  afterEach(() => {
    queryClient.clear();
  });

  it('hydrates auth store from /v1/auth/me', async () => {
    let authStore!: ReturnType<typeof useAuthStore>;

    mount(
      defineComponent({
        setup() {
          useAuthSession();
          authStore = useAuthStore();
          return {};
        },
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }], createPinia()],
        },
      },
    );

    await flushPromises();

    expect(authStore.user).toEqual(mockUser);
  });

  it('logout clears store and redirects to /signin', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    let logoutFn!: () => Promise<void>;

    mount(
      defineComponent({
        setup() {
          ({ logout: logoutFn } = useAuthSession());
          return {};
        },
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }], createPinia()],
        },
      },
    );

    await flushPromises();
    await logoutFn();
    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith('/signin');
    assignSpy.mockRestore();
  });

  it('does not fetch /me when guard is false', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});

    mount(
      defineComponent({
        setup() {
          useAuthSession({ guard: false });
          return {};
        },
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }], createPinia()],
        },
      },
    );

    await flushPromises();

    const meCalls = fetchMock.mock.calls.filter((call) =>
      call[0].url.includes('/v1/auth/me'),
    );
    expect(meCalls).toHaveLength(0);
    expect(assignSpy).not.toHaveBeenCalled();
    assignSpy.mockRestore();
  });

  it('redirects unauthenticated users when guard is enabled', async () => {
    fetchMock.mockImplementation(async (input: Request) => {
      if (input.method.toUpperCase() === 'GET' && input.url.includes('/v1/auth/me')) {
        return new Response(JSON.stringify({ message: 'unauthorized' }), {
          status: 401,
          headers: { 'Content-Type': 'application/json' },
        });
      }

      return new Response(null, { status: 204 });
    });

    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});

    mount(
      defineComponent({
        setup() {
          useAuthSession({ guard: true });
          return {};
        },
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }], createPinia()],
        },
      },
    );

    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith('/signin');
    assignSpy.mockRestore();
  });

  it('does not redirect on auth error when already on /signin', async () => {
    mockPagePathname.value = '/signin';

    fetchMock.mockImplementation(async (input: Request) => {
      if (input.method.toUpperCase() === 'GET' && input.url.includes('/v1/auth/me')) {
        return new Response(JSON.stringify({ message: 'unauthorized' }), {
          status: 401,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(null, { status: 204 });
    });

    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});

    mount(
      defineComponent({
        setup() {
          useAuthSession({ guard: true });
          return {};
        },
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }], createPinia()],
        },
      },
    );

    await flushPromises();

    expect(assignSpy).not.toHaveBeenCalled();
    assignSpy.mockRestore();
    mockPagePathname.value = '/';
  });
});

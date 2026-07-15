import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defineComponent, ref } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import {
  AUTH_ME_STALE_TIME_MS,
  createTestApiClient,
  fetchClient,
  meQueryOptions,
  projectLabelsQueryOptions,
  projectsQueryOptions,
  useLoginMutation,
  useLogoutMutation,
  useMeQuery,
  useRegisterMutation,
} from '../api-vue-query';

const mockUser = {
  id: '00000000-0000-0000-0000-000000000001',
  email: 'test@example.com',
  username: 'testuser',
  email_verified: true,
  is_admin: false,
  is_suspended: false,
  totp_enabled: false,
};

function createFetchMock() {
  return vi.fn(async (input: Request) => {
    const url = input.url;
    const method = input.method.toUpperCase();

    if (method === 'GET' && url.includes('/v1/auth/me')) {
      return new Response(JSON.stringify(mockUser), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    }

    if (method === 'POST' && url.includes('/v1/auth/login')) {
      return new Response(null, { status: 204 });
    }

    if (method === 'POST' && url.includes('/v1/auth/register')) {
      return new Response(JSON.stringify('Register successful'), {
        status: 201,
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
}

describe('api-vue-query PoC', () => {
  let queryClient: QueryClient;
  let fetchMock: ReturnType<typeof createFetchMock>;
  let testApi: ReturnType<typeof createTestApiClient>;

  beforeEach(() => {
    fetchMock = createFetchMock();
    testApi = createTestApiClient(fetchMock);
    queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
  });

  afterEach(() => {
    queryClient.clear();
  });

  function withQuery<T>(setup: () => T): T {
    let result!: T;
    mount(
      defineComponent({
        setup() {
          result = setup();
          return {};
        },
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }]],
        },
      },
    );
    return result;
  }

  it('useQuery fetches /v1/auth/me via mocked fetch', async () => {
    const query = withQuery(() => testApi.useQuery('get', '/v1/auth/me'));

    await flushPromises();

    expect(query.isSuccess.value).toBe(true);
    expect(query.data.value).toEqual(mockUser);
    expect(fetchMock).toHaveBeenCalled();
  });

  it('meQueryOptions sets staleTime for session cache', () => {
    const options = meQueryOptions();
    expect(options.staleTime).toBe(AUTH_ME_STALE_TIME_MS);
    expect(options.retry).toBe(false);
  });

  it('query key follows [method, path, init] shape', () => {
    const key = meQueryOptions().queryKey;
    expect(key).toEqual(['get', '/v1/auth/me']);

    const keyWithInit = projectLabelsQueryOptions('tenant-1', 'project-1').queryKey;
    expect(keyWithInit).toEqual([
      'get',
      '/v1/tenants/{tenant_id}/projects/{project_id}/labels',
      { params: { path: { tenant_id: 'tenant-1', project_id: 'project-1' } } },
    ]);
  });

  it('projectsQueryOptions builds tenant projects query key', () => {
    const options = projectsQueryOptions('tenant-uuid-1');
    expect(options.queryKey).toEqual([
      'get',
      '/v1/tenants/{tenant_id}/projects',
      { params: { path: { tenant_id: 'tenant-uuid-1' } } },
    ]);
  });

  it('useMutation posts to /v1/auth/login via mocked fetch', async () => {
    const mutation = withQuery(() => testApi.useMutation('post', '/v1/auth/login'));

    await mutation.mutateAsync({
      body: { email: 'test@example.com', password: 'password123' },
    } as never);
    await flushPromises();

    const loginCall = fetchMock.mock.calls.find((call) => call[0].url.includes('/v1/auth/login'));
    expect(loginCall?.[0].method).toBe('POST');
  });

  it('useMutation posts to /v1/auth/register via mocked fetch', async () => {
    const mutation = withQuery(() => testApi.useMutation('post', '/v1/auth/register'));

    await mutation.mutateAsync({
      body: {
        username: 'testuser',
        email: 'test@example.com',
        password: 'password123',
      },
      parseAs: 'text',
    } as never);
    await flushPromises();

    const registerCall = fetchMock.mock.calls.find((call) =>
      call[0].url.includes('/v1/auth/register'),
    );
    expect(registerCall?.[0].method).toBe('POST');
  });

  it('useMutation posts to /v1/auth/logout via mocked fetch', async () => {
    const mutation = withQuery(() => testApi.useMutation('post', '/v1/auth/logout'));

    await mutation.mutateAsync({} as never);
    await flushPromises();

    const logoutCall = fetchMock.mock.calls.find((call) => call[0].url.includes('/v1/auth/logout'));
    expect(logoutCall?.[0].method).toBe('POST');
  });

  it('useQuery sets isError on non-2xx response', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ message: 'unauthorized' }), {
        status: 401,
        headers: { 'Content-Type': 'application/json' },
      }),
    );

    const query = withQuery(() => testApi.useQuery('get', '/v1/auth/me'));

    await flushPromises();

    expect(query.isError.value).toBe(true);
  });

  it('useMutation sets error on non-2xx response', async () => {
    const mutation = withQuery(() => testApi.useMutation('post', '/v1/auth/logout'));

    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ message: 'forbidden' }), {
        status: 403,
        headers: { 'Content-Type': 'application/json' },
      }),
    );

    await expect(mutation.mutateAsync({} as never)).rejects.toBeTruthy();
  });
});

describe('api-vue-query production client', () => {
  let queryClient: QueryClient;
  const originalFetch = globalThis.fetch;

  function meResponse() {
    return new Response(JSON.stringify(mockUser), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  function withQuery<T>(setup: () => T): T {
    let result!: T;
    mount(
      defineComponent({
        setup() {
          result = setup();
          return {};
        },
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }]],
        },
      },
    );
    return result;
  }

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
  });

  afterEach(() => {
    queryClient.clear();
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it('fetchClient delegates HTTP to globalThis.fetch', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(meResponse());
    globalThis.fetch = fetchSpy;

    const { data, error } = await fetchClient.GET('/v1/auth/me');

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(error).toBeUndefined();
    expect(data).toEqual(mockUser);
  });

  it('useMeQuery fetches /v1/auth/me via production client', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(meResponse());
    globalThis.fetch = fetchSpy;

    const query = withQuery(() => useMeQuery());

    await flushPromises();

    expect(query.isSuccess.value).toBe(true);
    expect(query.data.value).toEqual(mockUser);
    expect(fetchSpy).toHaveBeenCalled();
  });

  it('useMeQuery does not fetch when enabled is false', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(meResponse());
    globalThis.fetch = fetchSpy;

    const query = withQuery(() => useMeQuery({ enabled: false }));

    await flushPromises();

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(query.isFetched.value).toBe(false);
    expect(query.data.value).toBeUndefined();
  });

  it('useMeQuery respects reactive enabled ref', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(meResponse());
    globalThis.fetch = fetchSpy;
    const enabled = ref(false);

    const query = withQuery(() => useMeQuery({ enabled }));

    await flushPromises();
    expect(fetchSpy).not.toHaveBeenCalled();

    enabled.value = true;
    await flushPromises();

    expect(fetchSpy).toHaveBeenCalled();
    expect(query.isSuccess.value).toBe(true);
    expect(query.data.value).toEqual(mockUser);
  });

  it('projectLabelsQueryOptions builds tenant/project query key', () => {
    const options = projectLabelsQueryOptions('tenant-a', 'project-b');
    expect(options.queryKey).toEqual([
      'get',
      '/v1/tenants/{tenant_id}/projects/{project_id}/labels',
      { params: { path: { tenant_id: 'tenant-a', project_id: 'project-b' } } },
    ]);
  });

  it('meQueryOptions disables retry for session cache', () => {
    expect(meQueryOptions().retry).toBe(false);
    expect(meQueryOptions().staleTime).toBe(AUTH_ME_STALE_TIME_MS);
  });

  it('useLoginMutation posts to /v1/auth/login via production wrapper', async () => {
    const fetchSpy = vi.fn<typeof fetch>().mockImplementation(async (input) => {
      const req = input instanceof Request ? input : new Request(input);
      if (req.method.toUpperCase() === 'POST' && req.url.includes('/v1/auth/login')) {
        return new Response(null, { status: 204 });
      }
      return new Response(JSON.stringify({ message: 'not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    });
    globalThis.fetch = fetchSpy;

    const mutation = withQuery(() => useLoginMutation());

    await mutation.mutateAsync({
      body: { email: 'test@example.com', password: 'password123' },
    } as never);
    await flushPromises();

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const loginReq =
      fetchSpy.mock.calls[0][0] instanceof Request
        ? fetchSpy.mock.calls[0][0]
        : new Request(fetchSpy.mock.calls[0][0]);
    expect(loginReq.method).toBe('POST');
    expect(loginReq.url).toContain('/v1/auth/login');
  });

  it('useRegisterMutation posts to /v1/auth/register via production wrapper', async () => {
    const fetchSpy = vi.fn<typeof fetch>().mockImplementation(async (input) => {
      const req = input instanceof Request ? input : new Request(input);
      if (req.method.toUpperCase() === 'POST' && req.url.includes('/v1/auth/register')) {
        return new Response(JSON.stringify('Register successful'), {
          status: 201,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      return new Response(JSON.stringify({ message: 'not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    });
    globalThis.fetch = fetchSpy;

    const mutation = withQuery(() => useRegisterMutation());

    await mutation.mutateAsync({
      body: {
        username: 'testuser',
        email: 'test@example.com',
        password: 'password123',
      },
      parseAs: 'text',
    } as never);
    await flushPromises();

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const registerReq =
      fetchSpy.mock.calls[0][0] instanceof Request
        ? fetchSpy.mock.calls[0][0]
        : new Request(fetchSpy.mock.calls[0][0]);
    expect(registerReq.method).toBe('POST');
    expect(registerReq.url).toContain('/v1/auth/register');
  });

  it('useLogoutMutation posts to /v1/auth/logout via production wrapper', async () => {
    const fetchSpy = vi.fn<typeof fetch>().mockImplementation(async (input) => {
      const req = input instanceof Request ? input : new Request(input);
      if (req.method.toUpperCase() === 'POST' && req.url.includes('/v1/auth/logout')) {
        return new Response(null, { status: 204 });
      }
      return new Response(JSON.stringify({ message: 'not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    });
    globalThis.fetch = fetchSpy;

    const mutation = withQuery(() => useLogoutMutation());

    await mutation.mutateAsync({} as never);
    await flushPromises();

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const logoutReq =
      fetchSpy.mock.calls[0][0] instanceof Request
        ? fetchSpy.mock.calls[0][0]
        : new Request(fetchSpy.mock.calls[0][0]);
    expect(logoutReq.method).toBe('POST');
    expect(logoutReq.url).toContain('/v1/auth/logout');
  });
});

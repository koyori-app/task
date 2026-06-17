import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defineComponent } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import { createTestApiClient, meQueryOptions, projectLabelsQueryOptions } from '../api-vue-query';

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

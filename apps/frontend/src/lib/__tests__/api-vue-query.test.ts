import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defineComponent } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import {
  createTestApiVueQueryClient,
  meQueryOptions,
  projectLabelsQueryOptions,
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
  let testApi: ReturnType<typeof createTestApiVueQueryClient>;

  beforeEach(() => {
    fetchMock = createFetchMock();
    testApi = createTestApiVueQueryClient(fetchMock);
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

  function mountWithQuery(setup: () => void) {
    return mount(
      defineComponent({
        setup,
        template: '<div />',
      }),
      {
        global: {
          plugins: [[VueQueryPlugin, { queryClient }]],
        },
      },
    );
  }

  it('useQuery fetches /v1/auth/me via mocked fetch', async () => {
    const state = { queryResult: undefined as ReturnType<typeof testApi.useMeQuery> | undefined };

    mountWithQuery(() => {
      state.queryResult = testApi.useMeQuery();
    });

    await flushPromises();

    expect(state.queryResult!.isSuccess.value).toBe(true);
    expect(state.queryResult!.data.value).toEqual(mockUser);
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
    const state = {
      mutationResult: undefined as ReturnType<typeof testApi.useLogoutMutation> | undefined,
    };

    mountWithQuery(() => {
      state.mutationResult = testApi.useLogoutMutation();
    });

    await state.mutationResult!.mutateAsync({});
    await flushPromises();

    const logoutCall = fetchMock.mock.calls.find((call) => call[0].url.includes('/v1/auth/logout'));
    expect(logoutCall?.[0].method).toBe('POST');
  });
});

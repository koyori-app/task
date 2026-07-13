import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { computed, defineComponent } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const tenants = [
  {
    id: TENANT_UUID,
    display_id: 'acme',
    name: 'Acme',
    description: '',
    icon_url: '',
    owner_id: '00000000-0000-0000-0000-000000000002',
    require_2fa: false,
  },
];

const { getMock } = vi.hoisted(() => ({
  getMock: vi.fn(async () => ({ data: tenants, error: undefined })),
}));

vi.mock('@/lib/api-vue-query', () => ({
  fetchClient: {
    GET: getMock,
  },
}));

import { useResolvedTenantId } from '../useResolvedTenantId';

function mountComposable(displayId: string) {
  let result!: ReturnType<typeof useResolvedTenantId>;
  const Comp = defineComponent({
    setup() {
      result = useResolvedTenantId(computed(() => displayId));
      return () => null;
    },
  });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  mount(Comp, {
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
  });
  return { get: () => result, flush: () => flushPromises(), queryClient };
}

describe('useResolvedTenantId', () => {
  beforeEach(() => {
    getMock.mockClear();
    getMock.mockResolvedValue({ data: tenants, error: undefined });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('display_id から tenant UUID を解決する', async () => {
    const { get, flush } = mountComposable('acme');
    await flush();
    expect(get().tenantId.value).toBe(TENANT_UUID);
    expect(get().isTenantNotFound.value).toBe(false);
    expect(getMock).toHaveBeenCalled();
  });

  it('未知の display_id は not-found と判定する', async () => {
    const { get, flush } = mountComposable('unknown-tenant');
    await flush();
    expect(get().tenantId.value).toBeNull();
    expect(get().isTenantNotFound.value).toBe(true);
    expect(get().isError.value).toBe(false);
  });

  it('GET /v1/tenants 失敗時は isError を立てる', async () => {
    getMock.mockImplementation(async () => {
      throw { status: 500, message: 'server error' };
    });
    const { get, flush } = mountComposable('acme');
    await flush();
    expect(get().tenantId.value).toBeNull();
    expect(get().isTenantNotFound.value).toBe(false);
    expect(get().isError.value).toBe(true);
  });
});

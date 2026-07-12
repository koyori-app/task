import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from '@/lib/api';
import { useTenantStore, type Tenant } from '@/stores/tenant';

vi.mock('@/lib/api', () => ({
  apiClient: { GET: vi.fn() },
}));

const tenants: Tenant[] = [
  {
    id: 'tenant-1',
    display_id: 'alpha',
    name: 'Alpha',
    description: '',
    icon_url: '',
    owner_id: 'owner-1',
    require_2fa: false,
  },
  {
    id: 'tenant-2',
    display_id: 'beta',
    name: 'Beta',
    description: '',
    icon_url: '',
    owner_id: 'owner-1',
    require_2fa: false,
  },
];

describe('tenant store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(apiClient.GET).mockReset();
  });

  it('loads /v1/tenants and selects the tenant from the route', async () => {
    vi.mocked(apiClient.GET).mockResolvedValue({ data: tenants, response: new Response() });
    const store = useTenantStore();

    await store.loadTenants('beta');

    expect(apiClient.GET).toHaveBeenCalledWith('/v1/tenants');
    expect(store.tenants).toEqual(tenants);
    expect(store.selectedTenant?.display_id).toBe('beta');
    expect(store.error).toBeNull();
  });

  it('uses the first tenant when there is no route or persisted selection', async () => {
    vi.mocked(apiClient.GET).mockResolvedValue({ data: tenants, response: new Response() });
    const store = useTenantStore();

    await store.loadTenants();

    expect(store.selectedTenantId).toBe('tenant-1');
  });

  it('represents an empty tenant membership without throwing', async () => {
    vi.mocked(apiClient.GET).mockResolvedValue({ data: [], response: new Response() });
    const store = useTenantStore();

    await store.loadTenants();

    expect(store.tenants).toEqual([]);
    expect(store.selectedTenantId).toBeNull();
    expect(store.error).toBeNull();
  });

  it('clears tenant context and exposes a fallback message on API failure', async () => {
    vi.mocked(apiClient.GET).mockResolvedValue({
      error: { message: 'unauthorized' },
      response: new Response(null, { status: 401 }),
    });
    const store = useTenantStore();

    await store.loadTenants('alpha');

    expect(store.tenants).toEqual([]);
    expect(store.selectedTenantId).toBeNull();
    expect(store.error).toBe('テナント一覧を取得できませんでした');
  });
});

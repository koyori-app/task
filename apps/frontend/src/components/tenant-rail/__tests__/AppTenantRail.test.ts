import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { useAuthStore } from '@/stores/auth';
import type { Tenant } from '@/stores/tenant';
import AppTenantRail from '../AppTenantRail.vue';

const { tenantStoreStub } = vi.hoisted(() => ({
  tenantStoreStub: {
    tenants: [] as unknown[],
    selectedTenantId: null as string | null,
    selectedTenant: null as unknown,
    isLoading: false,
    error: null as string | null,
    loadTenants: vi.fn(),
    selectTenant: vi.fn(),
  },
}));

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('@/stores/tenant', async (orig) => ({
  ...(await orig<typeof import('@/stores/tenant')>()),
  useTenantStore: () => tenantStoreStub,
}));

enableAutoUnmount(afterEach);

const alpha: Tenant = {
  id: 'tenant-1',
  display_id: 'alpha',
  name: 'Alpha',
  description: '',
  icon_url: '',
  owner_id: 'owner-1',
  require_2fa: false,
  membership: 'Owner',
};
const beta: Tenant = {
  ...alpha,
  id: 'tenant-2',
  display_id: 'beta',
  name: 'Beta',
  membership: 'Guest',
};

/**
 * レールの表示（TenantRail）は ui-vue 0.15.0 の公開を待っているので、
 * 配線は defineExpose した関数から呼ぶ。
 */
type RailApi = {
  selectTenant: (tenant: Tenant) => void;
  retry: () => void;
  openCreateDialog: () => void;
  openMenu: (tenant: Tenant, position: { x: number; y: number }) => void;
};

function mountRail(routeTenant = 'alpha') {
  const pinia = createPinia();
  setActivePinia(pinia);
  useAuthStore().setUser({
    id: 'owner-1',
    email: 'owner@example.com',
    username: 'owner',
    email_verified: true,
    is_admin: false,
    is_suspended: false,
    totp_enabled: false,
  });
  const wrapper = mount(AppTenantRail, {
    attachTo: document.body,
    global: {
      plugins: [
        pinia,
        [
          VueQueryPlugin,
          { queryClient: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        ],
      ],
      provide: {
        'vike-vue:usePageContext': {
          urlPathname: `/${routeTenant}`,
          routeParams: routeTenant ? { tenant: routeTenant } : {},
        },
      },
      stubs: { CreateTenantDialog: true },
    },
  });
  return { wrapper, api: wrapper.vm as unknown as RailApi };
}

describe('AppTenantRail', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    );
    tenantStoreStub.tenants = [alpha, beta];
    tenantStoreStub.selectedTenantId = alpha.id;
    tenantStoreStub.selectedTenant = alpha;
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('loads the tenants of the URL', () => {
    mountRail('alpha');

    expect(tenantStoreStub.loadTenants).toHaveBeenCalledExactlyOnceWith('alpha');
  });

  it('switches to another tenant with a full page load', () => {
    const assign = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const { wrapper, api } = mountRail('alpha');

    api.selectTenant(beta);

    expect(tenantStoreStub.selectTenant).toHaveBeenCalledWith(beta);
    expect(assign).toHaveBeenCalledWith('/beta');
    // 狭い画面では Drawer を閉じる
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('does not reload the page for the current tenant', () => {
    const assign = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const { api } = mountRail('alpha');

    api.selectTenant(alpha);

    expect(tenantStoreStub.selectTenant).toHaveBeenCalledWith(alpha);
    expect(assign).not.toHaveBeenCalled();
  });

  it('retries loading the tenants of the URL', () => {
    const { api } = mountRail('alpha');
    tenantStoreStub.loadTenants.mockClear();

    api.retry();

    expect(tenantStoreStub.loadTenants).toHaveBeenCalledExactlyOnceWith('alpha');
  });

  it('opens the create dialog after closing the drawer', async () => {
    const { wrapper, api } = mountRail('alpha');
    const dialog = wrapper.findComponent({ name: 'CreateTenantDialog' });
    expect(dialog.props('open')).toBe(false);

    api.openCreateDialog();
    await flushPromises();

    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(dialog.props('open')).toBe(true);
  });

  it('opens the menu of the tenant with the items it allows', async () => {
    const { api } = mountRail('alpha');

    api.openMenu(alpha, { x: 10, y: 20 });
    await flushPromises();
    const alphaMenu = document.body.querySelector('[role="menu"][aria-label="Alpha のメニュー"]');
    expect(alphaMenu).not.toBeNull();
    expect(
      [...alphaMenu!.querySelectorAll('[role="menuitem"]:not([hidden])')].map((i) =>
        i.textContent?.trim(),
      ),
    ).toEqual(['テナント設定', 'メンバー', '表示 ID をコピー']);

    // Guest のテナントにはメンバーへの導線を出さない。持ち主でもないので設定も出さない
    api.openMenu({ ...beta, owner_id: 'someone-else' }, { x: 10, y: 20 });
    await flushPromises();
    const betaMenu = document.body.querySelector('[role="menu"][aria-label="Beta のメニュー"]');
    expect(
      [...betaMenu!.querySelectorAll('[role="menuitem"]:not([hidden])')].map((i) =>
        i.textContent?.trim(),
      ),
    ).toEqual(['表示 ID をコピー']);
  });
});

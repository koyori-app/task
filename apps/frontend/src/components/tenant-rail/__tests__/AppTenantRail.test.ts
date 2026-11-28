import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { useAuthStore } from '@/stores/auth';
import type { Tenant } from '@/stores/tenant';
import AppTenantRail from '../AppTenantRail.vue';
import TenantRail from '../TenantRail.vue';

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

/** レールの表示（TenantRail）は stub にし、出た出来事の配線だけを見る */
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
      stubs: { CreateTenantDialog: true, TenantRail: true },
    },
  });
  const rail = wrapper.findComponent(TenantRail);
  return { wrapper, rail };
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

  it('loads the tenants of the URL and passes the store state to the rail', () => {
    const { rail } = mountRail('alpha');

    expect(tenantStoreStub.loadTenants).toHaveBeenCalledExactlyOnceWith('alpha');
    expect(rail.props()).toMatchObject({
      tenants: [alpha, beta],
      selectedTenantId: alpha.id,
      loading: false,
      error: null,
      menuTenantId: null,
    });
  });

  it('switches to another tenant with a full page load', () => {
    const assign = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const { wrapper, rail } = mountRail('alpha');

    rail.vm.$emit('select', beta);

    expect(tenantStoreStub.selectTenant).toHaveBeenCalledWith(beta);
    expect(assign).toHaveBeenCalledWith('/beta');
    // 狭い画面では Drawer を閉じる
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('does not reload the page for the current tenant', () => {
    const assign = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const { rail } = mountRail('alpha');

    rail.vm.$emit('select', alpha);

    expect(tenantStoreStub.selectTenant).toHaveBeenCalledWith(alpha);
    expect(assign).not.toHaveBeenCalled();
  });

  it('retries loading the tenants of the URL', () => {
    const { rail } = mountRail('alpha');
    tenantStoreStub.loadTenants.mockClear();

    rail.vm.$emit('retry');

    expect(tenantStoreStub.loadTenants).toHaveBeenCalledExactlyOnceWith('alpha');
  });

  it('opens the create dialog after closing the drawer', async () => {
    const { wrapper, rail } = mountRail('alpha');
    const dialog = wrapper.findComponent({ name: 'CreateTenantDialog' });
    expect(dialog.props('open')).toBe(false);

    rail.vm.$emit('create');
    await flushPromises();

    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(dialog.props('open')).toBe(true);
  });

  it('opens the menu of the tenant with the items it allows', async () => {
    const { rail } = mountRail('alpha');

    rail.vm.$emit('menu', alpha, { x: 10, y: 20 });
    await flushPromises();
    const alphaMenu = document.body.querySelector('[role="menu"][aria-label="Alpha のメニュー"]');
    expect(alphaMenu).not.toBeNull();
    expect(
      [...alphaMenu!.querySelectorAll('[role="menuitem"]:not([hidden])')].map((i) =>
        i.textContent?.trim(),
      ),
    ).toEqual(['テナント設定', 'メンバー', '表示 ID をコピー']);
    // メニューを開いている項目のツールチップを止める
    expect(rail.props('menuTenantId')).toBe(alpha.id);

    // Guest のテナントにはメンバーへの導線を出さない。持ち主でもないので設定も出さない
    rail.vm.$emit('menu', { ...beta, owner_id: 'someone-else' }, { x: 10, y: 20 });
    await flushPromises();
    const betaMenu = document.body.querySelector('[role="menu"][aria-label="Beta のメニュー"]');
    expect(
      [...betaMenu!.querySelectorAll('[role="menuitem"]:not([hidden])')].map((i) =>
        i.textContent?.trim(),
      ),
    ).toEqual(['表示 ID をコピー']);
  });

  // コピーでも閉じる。Drawer を開いたままだと toast が Drawer の裏に隠れる
  it.each(['メンバー', '表示 ID をコピー'])(
    'closes the drawer when %s is chosen',
    async (label) => {
      const { wrapper, rail } = mountRail('alpha');
      rail.vm.$emit('menu', alpha, { x: 10, y: 20 });
      await flushPromises();

      const item = [
        ...document.body.querySelectorAll<HTMLElement>(
          '[role="menu"][aria-label="Alpha のメニュー"] [role="menuitem"]:not([hidden])',
        ),
      ].find((candidate) => candidate.textContent?.trim() === label);
      item!.click();
      await flushPromises();

      expect(wrapper.emitted('close')).toHaveLength(1);
    },
  );
});

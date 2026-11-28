import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount, type VueWrapper } from '@vue/test-utils';
import { useAuthStore } from '@/stores/auth';
import type { Tenant } from '@/stores/tenant';
import SidebarTenantHeader from '../SidebarTenantHeader.vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));

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

function mountHeader(
  props: { tenant: Tenant | null; label: string; rail?: boolean },
  userId = 'owner-1',
) {
  const pinia = createPinia();
  setActivePinia(pinia);
  useAuthStore().setUser({
    id: userId,
    email: 'owner@example.com',
    username: 'owner',
    email_verified: true,
    is_admin: false,
    is_suspended: false,
    totp_enabled: false,
  });
  return mount(SidebarTenantHeader, {
    props,
    global: {
      plugins: [
        pinia,
        [
          VueQueryPlugin,
          { queryClient: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        ],
      ],
      provide: {
        'vike-vue:usePageContext': { urlPathname: '/alpha', routeParams: { tenant: 'alpha' } },
      },
    },
  });
}

async function menuItems(wrapper: VueWrapper) {
  await wrapper.get('button[aria-label="テナントのメニュー"]').trigger('click');
  // 空のときの案内の項目は hidden で常に DOM にあるので除く
  return wrapper.findAll('[role^="menuitem"]:not([hidden])').map((item) => item.text());
}

describe('SidebarTenantHeader', () => {
  beforeEach(() =>
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    ),
  );
  afterEach(() => vi.unstubAllGlobals());

  it('shows the tenant name as a heading that does not switch tenants', () => {
    const wrapper = mountHeader({ tenant: alpha, label: 'Alpha' });

    const heading = wrapper.get('h2');
    expect(heading.text()).toBe('Alpha');
    expect(heading.find('button, a').exists()).toBe(false);
    expect(heading.element.closest('button, a')).toBeNull();
  });

  it('lists the menu items the user is allowed to use', async () => {
    expect(await menuItems(mountHeader({ tenant: alpha, label: 'Alpha' }))).toEqual([
      'テナント設定',
      'メンバー',
      '表示 ID をコピー',
    ]);
    expect(
      await menuItems(
        mountHeader({ tenant: { ...alpha, membership: 'Guest' }, label: 'Alpha' }, 'guest-1'),
      ),
    ).toEqual(['表示 ID をコピー']);
  });

  it('has no menu while the tenant is not decided', () => {
    const wrapper = mountHeader({ tenant: null, label: 'テナントを読み込み中…' });

    expect(wrapper.get('h2').text()).toBe('テナントを読み込み中…');
    expect(wrapper.find('button[aria-label="テナントのメニュー"]').exists()).toBe(false);
  });

  it('keeps the menu and reads the name when the sidebar is folded', () => {
    const wrapper = mountHeader({ tenant: alpha, label: 'Alpha', rail: true });

    expect(wrapper.get('h2').classes()).toContain('sr-only');
    expect(wrapper.find('button[aria-label="テナントのメニュー"]').exists()).toBe(true);
  });
});

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';

import AppHeader from '../AppHeader.vue';

vi.mock('@/composables/useAuthSession', () => ({ useAuthSession: () => ({ logout: vi.fn() }) }));
vi.mock('@/stores/tenant', async (orig) => {
  const mod = await orig<typeof import('@/stores/tenant')>();
  return {
    ...mod,
    useTenantStore: () => ({
      tenants: [],
      selectedTenantId: null,
      selectedTenant: null,
      isLoading: false,
      error: null,
      loadTenants: vi.fn(),
      selectTenant: vi.fn(),
    }),
  };
});

function mountHeader(urlPathname: string, routeParams: Record<string, string> = {}) {
  return mount(AppHeader, {
    global: {
      provide: { 'vike-vue:usePageContext': { urlPathname, routeParams } },
      stubs: { TenantSwitcher: true, NavUser: true },
    },
  });
}

const gear = (w: ReturnType<typeof mountHeader>) => w.find('a[aria-label="アカウント設定"]');

describe('AppHeader gear', () => {
  beforeEach(() => setActivePinia(createPinia()));

  it('links to account settings even outside a tenant and without ownership', () => {
    const link = gear(mountHeader('/'));

    expect(link.attributes('href')).toBe('/settings/profile');
    expect(link.attributes('title')).toBe('アカウント設定');
  });

  it('marks the gear under /settings only', () => {
    expect(gear(mountHeader('/settings/security')).classes()).toContain('bg-sidebar-accent');
    expect(gear(mountHeader('/alpha/settings', { tenant: 'alpha' })).classes()).not.toContain(
      'bg-sidebar-accent',
    );
    expect(gear(mountHeader('/settingsx')).classes()).not.toContain('bg-sidebar-accent');
  });
});

import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import type { Tenant } from '@/stores/tenant';
import SidebarProvider from '../../ui/sidebar/SidebarProvider.vue';
import TenantSwitcher from '../TenantSwitcher.vue';

enableAutoUnmount(afterEach);

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
];

describe('TenantSwitcher', () => {
  it('shows not-found instead of silently displaying the first tenant', () => {
    const wrapper = mount(
      {
        components: { SidebarProvider, TenantSwitcher },
        template:
          '<SidebarProvider><TenantSwitcher :tenants="tenants" :selected-tenant-id="null" /></SidebarProvider>',
        setup: () => ({ tenants }),
      },
      { global: { plugins: [createPinia()] } },
    );

    expect(wrapper.text()).toContain('指定されたテナントが見つかりません');
    expect(wrapper.text()).not.toContain('Alpha');
  });
});

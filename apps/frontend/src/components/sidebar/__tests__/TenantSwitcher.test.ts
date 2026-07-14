import { defineComponent } from 'vue';
import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
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
    icon_url: 'https://example.com/alpha.png',
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

const PassThrough = defineComponent({ template: '<div><slot /></div>' });
const Trigger = defineComponent({ template: '<div><slot /></div>' });
const Item = defineComponent({
  inheritAttrs: false,
  template: '<button v-bind="$attrs"><slot /></button>',
});
const ButtonStub = defineComponent({
  inheritAttrs: false,
  template: '<button v-bind="$attrs"><slot /></button>',
});

function mountSwitcher(props: {
  tenants: Tenant[];
  selectedTenantId: string | null;
  loading?: boolean;
  error?: string | null;
}) {
  return mount(
    {
      components: { SidebarProvider, TenantSwitcher },
      template: '<SidebarProvider><TenantSwitcher v-bind="props" /></SidebarProvider>',
      setup: () => ({ props }),
    },
    {
      global: {
        stubs: {
          DropdownMenu: PassThrough,
          DropdownMenuTrigger: Trigger,
          DropdownMenuContent: PassThrough,
          DropdownMenuLabel: PassThrough,
          DropdownMenuSeparator: PassThrough,
          DropdownMenuItem: Item,
          SidebarMenu: PassThrough,
          SidebarMenuItem: PassThrough,
          SidebarMenuButton: ButtonStub,
          CreateTenantDialog: true,
        },
      },
    },
  );
}

describe('TenantSwitcher', () => {
  it('shows all tenants and emits the selected tenant', async () => {
    const wrapper = mountSwitcher({ tenants, selectedTenantId: 'tenant-1' });

    expect(wrapper.text()).toContain('Alpha');
    expect(wrapper.text()).toContain('alpha');
    expect(wrapper.text()).toContain('Beta');

    const beta = wrapper.findAll('button').find((button) => button.text().includes('Beta'))!;
    await beta.trigger('click');

    expect(wrapper.findComponent(TenantSwitcher).emitted('select')).toEqual([[tenants[1]]]);
  });

  it('shows the empty state when the user has no tenant memberships', () => {
    const wrapper = mountSwitcher({ tenants: [], selectedTenantId: null });

    expect(wrapper.text()).toContain('所属テナントなし');
    expect(wrapper.text()).toContain('利用可能なテナントがありません');
    expect(wrapper.text()).toContain('所属テナントがありません');
  });

  it('shows a disabled loading state', () => {
    const wrapper = mountSwitcher({ tenants: [], selectedTenantId: null, loading: true });

    expect(wrapper.text()).toContain('テナントを読み込み中…');
    expect(wrapper.find('button[disabled]').exists()).toBe(true);
  });

  it('shows an API error and emits retry when it is selected', async () => {
    const wrapper = mountSwitcher({
      tenants: [],
      selectedTenantId: null,
      error: 'テナント一覧を取得できませんでした',
    });

    const retry = wrapper
      .findAll('button')
      .find((button) => button.text().includes('テナント一覧を取得できませんでした'))!;
    expect(retry.text()).toContain('再試行');
    await retry.trigger('click');

    expect(wrapper.findComponent(TenantSwitcher).emitted('retry')).toEqual([[]]);
  });

  it('shows not-found instead of silently displaying the first tenant', () => {
    const wrapper = mountSwitcher({ tenants, selectedTenantId: null });

    expect(wrapper.text()).toContain('指定されたテナントが見つかりません');
    expect(wrapper.text()).toContain('URLを確認してください');
  });
});

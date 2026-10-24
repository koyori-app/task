import { defineComponent, provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount, type VueWrapper } from '@vue/test-utils';
import { navigate } from 'vike/client/router';
import { useAuthStore } from '@/stores/auth';
import type { Tenant } from '@/stores/tenant';
import SidebarProvider from '../../ui/sidebar/SidebarProvider.vue';
import TenantSwitcher from '../TenantSwitcher.vue';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));

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
    membership: 'Owner' as const,
  },
  {
    id: 'tenant-2',
    display_id: 'beta',
    name: 'Beta',
    description: '',
    icon_url: '',
    owner_id: 'owner-1',
    require_2fa: false,
    membership: 'Owner' as const,
  },
];

const PassThrough = defineComponent({ template: '<div><slot /></div>' });
const ButtonStub = defineComponent({
  inheritAttrs: false,
  template: '<button v-bind="$attrs"><slot /></button>',
});

/** `authUserId` を渡すと、`/me` がまだ返っていない初回描画を再現できる。 */
function mountSwitcher(
  props: {
    tenants: Tenant[];
    selectedTenantId: string | null;
    loading?: boolean;
    error?: string | null;
  },
  authUserId?: string,
) {
  const pinia = createPinia();
  setActivePinia(pinia);
  if (authUserId) {
    useAuthStore().setUser({
      id: authUserId,
      email: 'owner@example.com',
      username: 'owner',
      email_verified: true,
      is_admin: false,
      is_suspended: false,
      totp_enabled: false,
    });
  }
  return mount(
    {
      components: { SidebarProvider, TenantSwitcher },
      template: '<SidebarProvider><TenantSwitcher v-bind="props" /></SidebarProvider>',
      setup: () => {
        provide(
          VUE_QUERY_CLIENT,
          new QueryClient({
            defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
          }),
        );
        return { props };
      },
    },
    {
      global: {
        plugins: [pinia],
        stubs: {
          SidebarMenu: PassThrough,
          SidebarMenuItem: PassThrough,
          SidebarMenuButton: ButtonStub,
          CreateTenantDialog: true,
        },
      },
    },
  );
}

/** 引き金を押して開いたメニューを返す。項目は閉じていても DOM にあるので、開いたことも確かめる */
async function openMenu(wrapper: VueWrapper) {
  await wrapper.get('button[aria-haspopup="menu"]').trigger('click');
  const menu = wrapper.get('[role="menu"]');
  expect(menu.attributes('hidden')).toBeUndefined();
  return menu;
}

function menuItem(wrapper: VueWrapper, name: string) {
  return wrapper.findAll('[role^="menuitem"]').find((item) => item.text().includes(name));
}

describe('TenantSwitcher', () => {
  it('opens the tenant menu and emits the selected tenant', async () => {
    const wrapper = mountSwitcher({ tenants, selectedTenantId: 'tenant-1' });

    expect(wrapper.get('button[aria-haspopup="menu"]').text()).toContain('Alpha');
    expect(wrapper.get('[role="menu"]').attributes('hidden')).toBeDefined();

    await openMenu(wrapper);
    // 今いるテナントに印が付く
    expect(menuItem(wrapper, 'Alpha')!.attributes('aria-checked')).toBe('true');
    const beta = menuItem(wrapper, 'Beta')!;
    expect(beta.attributes('role')).toBe('menuitemradio');
    expect(beta.attributes('aria-checked')).toBe('false');
    await beta.trigger('click');

    expect(wrapper.findComponent(TenantSwitcher).emitted('select')).toEqual([[tenants[1]]]);
  });

  it('shows the empty state when the user has no tenant memberships', async () => {
    const wrapper = mountSwitcher({ tenants: [], selectedTenantId: null });

    // ヘッダーのトリガーは 1 行なので、状態は主文で伝える
    expect(wrapper.text()).toContain('所属テナントなし');

    expect((await openMenu(wrapper)).text()).toContain('所属テナントがありません');
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

    await openMenu(wrapper);

    const retry = menuItem(wrapper, 'テナント一覧を取得できませんでした')!;
    expect(retry.text()).toContain('再試行');
    await retry.trigger('click');

    expect(wrapper.findComponent(TenantSwitcher).emitted('retry')).toEqual([[]]);
  });

  it('hides the members link from a project-only guest', async () => {
    const guest: Tenant = { ...tenants[0], membership: 'Guest' as const };
    const wrapper = mountSwitcher({ tenants: [guest], selectedTenantId: guest.id });

    await openMenu(wrapper);
    // 客分はメンバー一覧 API が 403 になるので、押せば必ず失敗する導線は出さない
    expect(menuItem(wrapper, 'メンバー')).toBeUndefined();
  });

  it('keeps the members link for a tenant member', async () => {
    const memberTenant: Tenant = { ...tenants[0], membership: 'Member' as const };
    const wrapper = mountSwitcher({ tenants: [memberTenant], selectedTenantId: memberTenant.id });

    await openMenu(wrapper);
    await menuItem(wrapper, 'メンバー')!.trigger('click');
    expect(navigate).toHaveBeenCalledWith('/alpha/settings/members');
  });

  it('keeps the tenant settings link before /me resolves', async () => {
    // 永続化された利用者だけが居る初回描画。AppHeader の歯車と出方を揃える
    const wrapper = mountSwitcher({ tenants, selectedTenantId: 'tenant-1' }, 'owner-1');

    await openMenu(wrapper);
    await menuItem(wrapper, 'テナント設定')!.trigger('click');
    expect(navigate).toHaveBeenCalledWith('/alpha/settings');
  });

  it('hides the tenant settings link from a non-owner', async () => {
    const wrapper = mountSwitcher({ tenants, selectedTenantId: 'tenant-1' }, 'member-1');

    await openMenu(wrapper);
    expect(menuItem(wrapper, 'テナント設定')).toBeUndefined();
  });

  it('shows not-found instead of silently displaying the first tenant', () => {
    const wrapper = mountSwitcher({ tenants, selectedTenantId: null });

    expect(wrapper.text()).toContain('指定されたテナントが見つかりません');
  });
});

import { defineComponent } from 'vue';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { toast } from '@koyori-app/ui-vue';
import { navigate } from 'vike/client/router';
import { useAuthStore } from '@/stores/auth';
import { useTenantStore, type Tenant } from '@/stores/tenant';
import { useTenantMenu } from '../useTenantMenu';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));
vi.mock('@koyori-app/ui-vue', async (orig) => ({
  ...(await orig<typeof import('@koyori-app/ui-vue')>()),
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), dismiss: vi.fn() },
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
const beta: Tenant = { ...alpha, id: 'tenant-2', display_id: 'beta', name: 'Beta' };

/** `authUserId` を渡すと、`/me` がまだ返っていない初回描画を再現できる。 */
function setup(tenant: Tenant | null, authUserId?: string, routeTenant = 'alpha') {
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
  let menu!: ReturnType<typeof useTenantMenu>;
  mount(
    defineComponent({
      setup() {
        menu = useTenantMenu(() => tenant);
        return () => null;
      },
    }),
    {
      global: {
        plugins: [
          pinia,
          [
            VueQueryPlugin,
            {
              queryClient: new QueryClient({
                defaultOptions: { queries: { retry: false, gcTime: 0 } },
              }),
            },
          ],
        ],
        provide: {
          'vike-vue:usePageContext': {
            urlPathname: `/${routeTenant}`,
            routeParams: { tenant: routeTenant },
          },
        },
      },
    },
  );
  return menu;
}

const values = (menu: ReturnType<typeof useTenantMenu>) => menu.items.value.map((i) => i.value);

describe('useTenantMenu', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      'fetch',
      vi.fn(() => new Promise(() => {})),
    );
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('has no items without a tenant', () => {
    expect(values(setup(null, 'owner-1'))).toEqual([]);
  });

  it('hides the members link from a project-only guest', () => {
    // Guest はメンバー一覧 API が 403 になるので、押せば必ず失敗する導線は出さない
    expect(values(setup({ ...alpha, membership: 'Guest' }, 'member-1'))).toEqual(['copy-id']);
  });

  it('keeps the members link for a tenant member', () => {
    expect(values(setup({ ...alpha, membership: 'Member' }, 'member-1'))).toEqual([
      'members',
      'copy-id',
    ]);
  });

  it('keeps the tenant settings link for the owner before /me resolves', () => {
    expect(values(setup(alpha, 'owner-1'))).toEqual(['settings', 'members', 'copy-id']);
  });

  it('hides the tenant settings link from a non-owner', () => {
    expect(values(setup(alpha, 'member-1'))).not.toContain('settings');
  });

  it('navigates within the app for the tenant in the URL', () => {
    const assign = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const menu = setup(alpha, 'owner-1');

    menu.onSelect('settings');
    menu.onSelect('members');

    expect(navigate).toHaveBeenNthCalledWith(1, '/alpha/settings');
    expect(navigate).toHaveBeenNthCalledWith(2, '/alpha/settings/members');
    expect(assign).not.toHaveBeenCalled();
  });

  it('switches the tenant and loads the page for another tenant', () => {
    const assign = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const menu = setup(beta, 'owner-1');

    menu.onSelect('settings');

    // テナントに紐づく状態を残さないため、store を更新してからフルページ遷移にする
    expect(useTenantStore().selectedTenantId).toBe('tenant-2');
    expect(assign).toHaveBeenCalledWith('/beta/settings');
    expect(navigate).not.toHaveBeenCalled();
  });

  it('copies the display id and says so', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });
    const menu = setup({ ...alpha, membership: 'Guest' }, 'member-1');

    menu.onSelect('copy-id');
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith('alpha');
    expect(toast.success).toHaveBeenCalledWith('コピーしました');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('says the copy failed when the clipboard rejects', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'));
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });
    const menu = setup(alpha, 'owner-1');

    menu.onSelect('copy-id');
    await flushPromises();

    expect(toast.error).toHaveBeenCalledWith('コピーできませんでした');
    expect(toast.success).not.toHaveBeenCalled();
  });

  it('says the copy failed without the clipboard API', async () => {
    // 非 secure context では navigator.clipboard が無い
    vi.stubGlobal('navigator', { ...navigator, clipboard: undefined });
    const menu = setup(alpha, 'owner-1');

    menu.onSelect('copy-id');
    await flushPromises();

    expect(toast.error).toHaveBeenCalledWith('コピーできませんでした');
  });
});

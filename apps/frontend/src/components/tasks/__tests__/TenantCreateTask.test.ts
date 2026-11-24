import { afterEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import TenantCreateTask from '../TenantCreateTask.vue';
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts';

enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

const tenant = {
  id: '00000000-0000-4000-8000-000000000001',
  display_id: 'acme',
  name: '会社',
  membership: 'Owner',
  description: '',
  icon_url: '',
};
const project = (id: string, key: string) => ({
  id,
  tenant_id: tenant.id,
  key,
  name: key,
  description: '',
  icon_emoji: null,
  icon_url: null,
  is_personal: false,
  personal_owner_id: null,
  inherit_tenant_members: true,
});
const koy = project('00000000-0000-4000-8000-000000000002', 'KOY');
const eng = project('00000000-0000-4000-8000-000000000003', 'ENG');

function setup(
  options: {
    projects?: unknown[];
    routeParams?: Record<string, string>;
    /** 同じ画面に自前の T を持つ画面を置く */
    withPrimary?: boolean;
  } = {},
) {
  const projects = options.projects ?? [koy, eng];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (request: Request) => {
      const pathname = new URL(request.url, 'http://localhost').pathname.replace(/^\/api/, '');
      const replies = new Map<string, unknown>([
        ['/v1/auth/me', { id: 'user-1', email: 'a@example.com', username: 'a' }],
        ['/v1/tenants', [tenant]],
        [`/v1/tenants/${tenant.id}/projects`, projects],
      ]);
      return Response.json(replies.get(pathname) ?? { message: 'Forbidden' }, {
        status: replies.has(pathname) ? 200 : 403,
      });
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const primary = vi.fn();
  const Page = defineComponent({
    setup() {
      useKeyboardShortcuts({ t: primary });
      return () => h('div');
    },
  });
  const Root = defineComponent({
    setup() {
      return () => [options.withPrimary ? h(Page) : null, h(TenantCreateTask)];
    },
  });
  const wrapper = mount(Root, {
    attachTo: document.body,
    global: {
      plugins: [[VueQueryPlugin, { queryClient: client }]],
      provide: {
        'vike-vue:usePageContext': {
          routeParams: { tenant: 'acme', ...options.routeParams },
        },
      },
      stubs: { DashboardCreateTask: true },
    },
  });
  return { wrapper, client, primary };
}

const pressT = () =>
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 't', cancelable: true }));
const creator = (wrapper: ReturnType<typeof setup>['wrapper']) =>
  wrapper.findComponent({ name: 'DashboardCreateTask' });

describe('TenantCreateTask', () => {
  it('T でプロジェクトを選ぶ作成ダイアログを開く。選べるのは共有プロジェクトの一覧', async () => {
    const { wrapper } = setup();
    await flushPromises();
    expect(creator(wrapper).props('open')).toBe(false);
    pressT();
    await flushPromises();
    expect(creator(wrapper).props()).toMatchObject({
      open: true,
      tenantId: tenant.id,
      projects: [koy, eng],
      currentUserId: 'user-1',
      initialProjectId: undefined,
    });
  });

  it('共有プロジェクトが 0 件なら置かず、T でも開かない', async () => {
    const { wrapper } = setup({ projects: [] });
    await flushPromises();
    pressT();
    await flushPromises();
    expect(creator(wrapper).exists()).toBe(false);
  });

  it('テナントが解決できなければ置かず、T でも開かない', async () => {
    const { wrapper } = setup({ routeParams: { tenant: 'unknown' } });
    await flushPromises();
    pressT();
    await flushPromises();
    expect(creator(wrapper).exists()).toBe(false);
  });

  it('プロジェクト配下のページでは、そのプロジェクトを選んだ状態で開く', async () => {
    const { wrapper } = setup({ routeParams: { projectKey: 'ENG' } });
    await flushPromises();
    expect(creator(wrapper).props('initialProjectId')).toBe(eng.id);
  });

  it('画面が自前の T を持つときは開かない', async () => {
    const { wrapper, primary } = setup({ withPrimary: true });
    await flushPromises();
    pressT();
    await flushPromises();
    expect(primary).toHaveBeenCalledOnce();
    expect(creator(wrapper).props('open')).toBe(false);
  });

  it.each([true, false])(
    '作成したらマイタスクとカレンダーを取り直す（keepOpen: %s）',
    async (keepOpen) => {
      const { wrapper, client } = setup();
      await flushPromises();
      const invalidate = vi.spyOn(client, 'invalidateQueries');
      creator(wrapper).vm.$emit('created', { id: 't', seq_id: 1 }, koy, { keepOpen });
      await flushPromises();
      expect(invalidate.mock.calls.map(([filters]) => filters)).toEqual([
        { queryKey: ['get', '/v1/tenants/{tenant_id}/users/me/tasks'] },
        { queryKey: ['get', '/v1/tenants/{tenant_id}/schedule'] },
      ]);
    },
  );
});

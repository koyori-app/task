import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { persistedPinia } from '@/components/settings/__tests__/test-pinia';

const pageContext = { urlPathname: '/settings/apps' };
vi.mock('vike-vue/usePageContext', () => ({ usePageContext: () => pageContext }));

import AppsPage from '../apps/+Page.vue';

const jsonResponse = (data: unknown) =>
  new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });

enableAutoUnmount(afterEach);
beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => {
      const pathname = new URL(req.url, 'http://localhost').pathname;
      if (pathname.endsWith('/v1/oauth_grants')) {
        return jsonResponse([
          {
            id: '22222222-2222-2222-2222-222222222222',
            client_id: 'https://claude.ai/oauth/mcp-oauth-client-metadata',
            client_name: 'Claude',
            tenant_ids: ['11111111-1111-1111-1111-111111111111'],
            scopes: ['read:task'],
            expires_at: '2099-11-14T00:00:00Z',
            last_used_at: null,
          },
        ]);
      }
      return jsonResponse([]);
    }),
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('/settings/apps の画面', () => {
  it('アカウント設定の一覧と連携中のアプリの節を出し、一覧の「連携中のアプリ」が現在の項目になる', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = mount(AppsPage, {
      attachTo: document.body,
      global: { plugins: [persistedPinia(), [VueQueryPlugin, { queryClient }]] },
    });
    await flushPromises();

    expect(wrapper.get('h1').text()).toBe('アカウント設定');
    expect(wrapper.get('h2').text()).toBe('連携中のアプリ');
    const current = wrapper.get('a[aria-current="page"]');
    expect(current.attributes('href')).toBe('/settings/apps');
    expect(current.text()).toBe('連携中のアプリ');
    expect(wrapper.findAll('[data-testid="connected-app-list"] > li')).toHaveLength(1);
    expect(wrapper.text()).toContain('Claude');
  });
});

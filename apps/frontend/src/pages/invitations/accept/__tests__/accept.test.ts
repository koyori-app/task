import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import Page from '../+Page.vue';

enableAutoUnmount(afterEach);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const preview = {
  email: 'invitee@example.com',
  expires_at: '2030-01-01T00:00:00Z',
  invited_by: 'owner',
  role: 'member',
  tenant_display_id: 'demo',
  tenant_id: '00000000-0000-0000-0000-000000000001',
  tenant_name: 'Demo',
};

function json(body: unknown, status: number) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function mountPage(meStatus: number) {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: Request) =>
      input.url.includes('/v1/invitations/preview')
        ? json(preview, 200)
        : json({ message: 'x' }, meStatus),
    ),
  );
  return mount(Page, {
    global: {
      plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]],
      provide: { 'vike-vue:usePageContext': { urlParsed: { search: { token: 'abc' } } } },
    },
  });
}

describe('/invitations/accept', () => {
  it('セッションが拒否されたらサインインを案内する', async () => {
    const wrapper = mountPage(401);
    await vi.advanceTimersByTimeAsync(0);
    await flushPromises();

    expect(wrapper.text()).toContain('サインインして参加');
  });

  it('/me のサーバーエラーではサインインを案内せず、エラーを出す', async () => {
    const wrapper = mountPage(502);
    await vi.advanceTimersByTimeAsync(10_000);
    await flushPromises();

    expect(wrapper.text()).not.toContain('サインインして参加');
    expect(wrapper.get('[role="alert"]').text()).toContain('サーバーに接続できませんでした');
  });
});

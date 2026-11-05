import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import Page from '../+Page.vue';

enableAutoUnmount(afterEach);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function mountPage(meStatus: number) {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(JSON.stringify({ message: 'x' }), {
          status: meStatus,
          headers: { 'Content-Type': 'application/json' },
        }),
    ),
  );
  return mount(Page, {
    global: {
      plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]],
      provide: {
        'vike-vue:usePageContext': {
          urlParsed: {
            search: {
              port: '43123',
              code_challenge: 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM',
              state: 'st',
              name: 'laptop',
            },
          },
        },
      },
    },
  });
}

describe('/desktop/authorize', () => {
  it('セッションが拒否されたらサインインへ送る', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});

    mountPage(401);
    await vi.advanceTimersByTimeAsync(0);
    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith('/signin');
  });

  it('/me のサーバーエラーではサインインへ送らず、エラーを出す', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});

    const wrapper = mountPage(502);
    await vi.advanceTimersByTimeAsync(10_000);
    await flushPromises();

    expect(assignSpy).not.toHaveBeenCalled();
    expect(wrapper.get('[role="alert"]').text()).toContain('サーバーに接続できませんでした');
  });
});

import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import { h } from 'vue';

import Layout from '../+Layout.vue';

enableAutoUnmount(afterEach);

const mockUser = {
  id: '00000000-0000-0000-0000-000000000001',
  email: 'test@example.com',
  username: 'testuser',
  email_verified: true,
  is_admin: false,
  is_suspended: false,
  totp_enabled: false,
};

// 認証ガードが有効なページでは /v1/auth/me が飛ぶ。応答は返さず未解決のままにして
// 「解決を待っている」状態を再現する
beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

// サインイン前に開くページは認証ガードを通さず、そのまま中身を描画する。
// ここから漏れると /signin へリダイレクトされ、メール確認・パスワード再設定が完了できない。
// /desktop/authorize は未ログインなら自分でサインインへ送る（戻り先を覚えるため）。
const PRE_AUTH_PATHS = [
  '/signin',
  '/signup',
  '/auth/reset-password',
  '/verify-email',
  '/desktop/authorize',
];

function newQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

function mountLayout(urlPathname: string, queryClient = newQueryClient()) {
  return mount(Layout, {
    slots: { default: () => h('div', 'ページ本体') },
    global: {
      plugins: [createPinia(), [VueQueryPlugin, { queryClient }]],
      provide: { 'vike-vue:usePageContext': { urlPathname, urlParsed: { search: {} } } },
      // vike のランタイム前提のコンポーネントは単体マウントできない
      stubs: {
        ClientOnly: true,
        AppTenantRail: true,
        AppSidebar: true,
        AppSidebarSkeleton: true,
        AppBreadcrumbs: true,
      },
    },
  });
}

/** 狭い画面（max-width: 768px）かどうかを matchMedia で決める */
function stubViewport(mobile: boolean) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn((query: string) => ({
      matches: mobile && query.includes('max-width: 768px'),
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  );
}

function stubSignedIn() {
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(JSON.stringify(mockUser), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
    ),
  );
}

describe('+Layout', () => {
  it.each(PRE_AUTH_PATHS)('%s は認証ガードを通さずページを描画する', async (path) => {
    const wrapper = mountLayout(path);
    await flushPromises();

    expect(wrapper.text()).toContain('ページ本体');
    expect(wrapper.text()).not.toContain('読み込み中…');
  });

  it('通常のページは認証の解決を待つ', async () => {
    const wrapper = mountLayout('/my-tasks');
    await flushPromises();

    expect(wrapper.text()).not.toContain('ページ本体');
  });

  it('ヘッダーを置かず、レールとサイドバーを本文の外に並べて本文だけをスクロールさせる', async () => {
    stubSignedIn();

    const wrapper = mountLayout('/my-tasks');
    await flushPromises();

    expect(wrapper.find('[data-slot="app-header"]').exists()).toBe(false);

    // 画面の高さに固定した外枠が、ナビと本文を横に並べる
    const scrollContainer = wrapper.get('main');
    const shell = scrollContainer.element.parentElement!;
    expect([...shell.classList]).toEqual(
      expect.arrayContaining(['flex', 'h-svh', 'overflow-hidden']),
    );
    expect([...shell.classList]).not.toContain('flex-col');
    expect(scrollContainer.classes()).toEqual(
      expect.arrayContaining(['min-h-0', 'min-w-0', 'overflow-y-auto']),
    );

    const rail = wrapper.get('app-tenant-rail-stub').element;
    expect(shell.contains(rail)).toBe(true);
    expect(scrollContainer.element.contains(rail)).toBe(false);
    // レールはサイドバーの左に並ぶ
    const navigation = wrapper.get('[data-slot="app-navigation"]').element;
    expect(navigation.firstElementChild).toBe(rail);
    expect(navigation.contains(wrapper.get('app-sidebar-stub').element)).toBe(true);
  });

  describe('本文の上の帯', () => {
    it('広い画面ではパンくずを出し、今のテナントのボタンは出さない', async () => {
      stubViewport(false);
      stubSignedIn();
      const wrapper = mountLayout('/my-tasks');
      await flushPromises();

      expect(wrapper.find('app-breadcrumbs-stub').exists()).toBe(true);
      expect(wrapper.find('[data-slot="current-tenant"]').exists()).toBe(false);
    });

    it('狭い画面ではパンくずの代わりに今のテナントを出し、押すと Drawer を開く', async () => {
      stubViewport(true);
      stubSignedIn();
      const wrapper = mountLayout('/my-tasks');
      await flushPromises();

      expect(wrapper.find('app-breadcrumbs-stub').exists()).toBe(false);
      const current = wrapper.get('[data-slot="current-tenant"]');
      expect(current.attributes('aria-haspopup')).toBe('dialog');
      expect(current.attributes('aria-controls')).toBe('app-sidebar');
      // テナント一覧はまだ無いので、サイドバーの見出しと同じ文言になる
      expect(current.text()).toBe('所属テナントなし');

      const toggle = wrapper.get('button[aria-label="サイドバーを開閉"]');
      expect(toggle.attributes('aria-expanded')).toBe('false');
      await current.trigger('click');
      expect(toggle.attributes('aria-expanded')).toBe('true');
    });
  });

  // 広い画面の開閉ボタンはアイコンだけの表示（rail）を切り替え、旧サイドバーと同じ cookie に残す。
  // 値の意味も引き継ぐ（sidebar_state=false が畳んだ状態）ので、既存の利用者の状態がそのまま効く
  describe('広い画面の開閉ボタン', () => {
    afterEach(() => {
      document.cookie = 'sidebar_state=; path=/; max-age=0';
    });

    it('rail を切り替えて cookie に残す', async () => {
      stubSignedIn();
      const wrapper = mountLayout('/my-tasks');
      await flushPromises();

      const toggle = wrapper.get('button[aria-label="サイドバーを開閉"]');
      expect(toggle.attributes('aria-controls')).toBe('app-sidebar');
      expect(toggle.attributes('aria-expanded')).toBe('true');

      await toggle.trigger('click');
      expect(toggle.attributes('aria-expanded')).toBe('false');
      expect(document.cookie).toContain('sidebar_state=false');

      await toggle.trigger('click');
      expect(toggle.attributes('aria-expanded')).toBe('true');
      expect(document.cookie).toContain('sidebar_state=true');
    });

    it('cookie が sidebar_state=false なら rail で開く', async () => {
      document.cookie = 'sidebar_state=false; path=/';
      stubSignedIn();
      const wrapper = mountLayout('/my-tasks');
      await flushPromises();

      const toggle = wrapper.get('button[aria-label="サイドバーを開閉"]');
      expect(toggle.attributes('aria-expanded')).toBe('false');
    });
  });

  it('メール未認証の案内は、/me の再取得が失敗しても維持する', async () => {
    const unverified = { ...mockUser, email_verified: false };
    let fail = false;
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        fail
          ? new Response(JSON.stringify({ message: 'boom' }), {
              status: 500,
              headers: { 'Content-Type': 'application/json' },
            })
          : new Response(JSON.stringify(unverified), {
              status: 200,
              headers: { 'Content-Type': 'application/json' },
            }),
      ),
    );
    const queryClient = newQueryClient();
    const wrapper = mountLayout('/my-tasks', queryClient);
    await flushPromises();
    expect(wrapper.text()).toContain(unverified.email);
    expect(wrapper.find('app-tenant-rail-stub').exists()).toBe(false);

    // 再試行の待ち（指数バックオフ）を飛ばして、最終的な失敗まで進める
    vi.useFakeTimers({ toFake: ['setTimeout'] });
    fail = true;
    void queryClient.refetchQueries({ queryKey: ['get', '/v1/auth/me'] });
    await vi.advanceTimersByTimeAsync(10_000);
    await flushPromises();
    vi.useRealTimers();

    expect(queryClient.getQueryState(['get', '/v1/auth/me'])?.status).toBe('error');
    expect(wrapper.text()).toContain(unverified.email);
    expect(wrapper.find('app-tenant-rail-stub').exists()).toBe(false);
  });

  // 2FA の途中や停止中のセッションは /me が 403 を返す。接続エラーと誤認して留めない
  it.each([
    [401, '/signin'],
    [403, expect.stringContaining('/auth/2fa')],
  ])('/me が %i なら再試行せずに移動し、接続エラーを出さない', async (status, destination) => {
    vi.useFakeTimers({ toFake: ['setTimeout'] });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const fetchMock = vi.fn(
      async () =>
        new Response(JSON.stringify({ message: 'rejected' }), {
          status,
          headers: { 'Content-Type': 'application/json' },
        }),
    );
    vi.stubGlobal('fetch', fetchMock);

    const wrapper = mountLayout('/my-tasks');
    await vi.advanceTimersByTimeAsync(10_000);
    vi.useRealTimers();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(assignSpy).toHaveBeenCalledWith(destination);
    expect(wrapper.text()).not.toContain('サーバーに接続できませんでした');
    assignSpy.mockRestore();
  });
});

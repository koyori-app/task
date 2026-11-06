// テストケース
// - 確認ダイアログが対象ユーザー（名前とメール）と影響を明記し、確認するまで API を呼ばないこと
// - 2FA リセット / パスキー削除 / OAuth 解除 / パスワードリセット / 停止 / 権限変更が正しい API を呼ぶこと
// - 失敗（403 / 404 / 409）が操作ごとの文言で出ること
// - 自分自身の降格・停止ボタンが無効なこと、管理者以外にはシェルが中身を出さないこと
import { describe, it, expect, afterEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import AdminUserDetail from '../AdminUserDetail.vue';
import AdminShell from '../AdminShell.vue';
import { adminErrorMessage } from '@/lib/admin-users';

vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({ urlPathname: '/admin/users' }),
}));

const ME = '00000000-0000-0000-0000-00000000000a';
const TARGET = '00000000-0000-0000-0000-00000000000b';
const PASSKEY = '00000000-0000-0000-0000-00000000000c';

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });

function userRow(id: string, over: Record<string, unknown> = {}) {
  return {
    id,
    username: id === ME ? 'root' : 'alice',
    email: id === ME ? 'root@example.com' : 'alice@example.com',
    email_verified: true,
    is_admin: id === ME,
    is_suspended: false,
    totp_enabled: id !== ME,
    has_password: true,
    ...over,
  };
}

function stubFetch(opts: { me?: { is_admin: boolean }; failWith?: number } = {}) {
  const calls: string[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => {
      const url = new URL(req.url, 'http://localhost');
      const key = `${req.method} ${url.pathname.replace(/^\/api/, '')}`;
      if (req.method === 'GET' && key.endsWith('/v1/auth/me')) {
        return json(userRow(ME, { is_admin: opts.me?.is_admin ?? true }));
      }
      if (req.method === 'GET' && key.endsWith('/v1/admin/users')) {
        return json([userRow(ME), userRow(TARGET)]);
      }
      if (req.method === 'GET' && key.endsWith('/auth-methods')) {
        return json({
          passkeys: [{ id: PASSKEY, name: 'YubiKey', created_at: '2026-09-01T00:00:00Z' }],
          oauth: [
            {
              provider: 'github',
              provider_login: 'octocat',
              created_at: '2026-09-01T00:00:00Z',
            },
          ],
        });
      }
      calls.push(key);
      if (opts.failWith) return json({ message: 'x' }, opts.failWith);
      return key.endsWith('/password-reset')
        ? json({ message: 'ok' })
        : new Response(null, { status: 204 });
    }),
  );
  return calls;
}

function mountDetail() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(AdminUserDetail, {
    props: { userId: TARGET },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

const button = (label: string) =>
  [...document.body.querySelectorAll('button')].find((b) => b.textContent?.trim() === label);
/** 同名のボタンが画面とダイアログにあるときは、後ろ（ダイアログ）を押す。 */
async function click(label: string, last = false) {
  const all = [...document.body.querySelectorAll('button')].filter(
    (b) => b.textContent?.trim() === label,
  );
  const b = last ? all.at(-1) : all[0];
  if (!b) throw new Error(`button "${label}" not found`);
  b.click();
  await flushPromises();
}

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = '';
});

describe('AdminUserDetail の確認ダイアログ', () => {
  it('2FA リセットは対象と影響を示し、確認するまで API を呼ばない', async () => {
    const calls = stubFetch();
    mountDetail();
    await flushPromises();

    await click('2FA をリセット');
    expect(document.body.textContent).toContain('alice（alice@example.com）');
    expect(document.body.textContent).toContain('セッションが失効');
    expect(calls).toEqual([]);

    await click('リセットする');
    expect(calls).toEqual([`POST /v1/admin/users/${TARGET}/reset-2fa`]);
    expect(document.body.textContent).toContain('2FA をリセットしました');
  });

  it('パスキー削除は passkey_id を指定して呼ぶ', async () => {
    const calls = stubFetch();
    mountDetail();
    await flushPromises();

    await click('削除');
    expect(document.body.textContent).toContain('YubiKey');
    await click('削除する');
    expect(calls).toEqual([`DELETE /v1/admin/users/${TARGET}/passkeys/${PASSKEY}`]);
  });

  it('OAuth 解除は provider を指定して呼ぶ', async () => {
    const calls = stubFetch();
    mountDetail();
    await flushPromises();

    await click('解除');
    expect(document.body.textContent).toContain('alice（alice@example.com）');
    await click('解除する');
    expect(calls).toEqual([`DELETE /v1/admin/users/${TARGET}/oauth/github`]);
  });

  it('停止は影響を示して PATCH する。キャンセルでは呼ばない', async () => {
    const calls = stubFetch();
    mountDetail();
    await flushPromises();

    await click('停止する');
    expect(document.body.textContent).toContain('アクセストークンが失効');
    await click('キャンセル');
    expect(calls).toEqual([]);

    await click('停止する');
    await click('停止する', true);
    expect(calls).toEqual([`PATCH /v1/admin/users/${TARGET}`]);
  });

  it('403 / 404 / 409 は操作ごとの文言で表示する', async () => {
    stubFetch({ failWith: 409 });
    mountDetail();
    await flushPromises();
    await click('パスワードリセットメールを送る');
    await click('送信する');
    expect(document.body.textContent).toContain('短時間に送りすぎています');
  });
});

describe('adminErrorMessage', () => {
  const err = (status: number) => ({ response: new Response(null, { status }) });
  it('同じ状態でも操作ごとに説明が違う', () => {
    expect(adminErrorMessage('changeAdmin', err(403))).toContain('最後の管理者は降格');
    expect(adminErrorMessage('suspend', err(403))).toContain('最後の管理者は停止');
    expect(adminErrorMessage('deletePasskey', err(404))).toContain('パスキー');
    expect(adminErrorMessage('disconnectOAuth', err(404))).toContain('OAuth 連携');
    expect(adminErrorMessage('createUser', err(409))).toContain('メールアドレスまたはユーザー名');
    expect(adminErrorMessage('reset2fa', err(500))).toContain('失敗');
  });
});

describe('管理者でない場合', () => {
  it('自分自身の降格・停止は無効', async () => {
    stubFetch();
    const queryClient = new QueryClient();
    const wrapper = mount(AdminUserDetail, {
      props: { userId: ME },
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      attachTo: document.body,
    });
    await flushPromises();
    expect((button('管理者権限を外す') as HTMLButtonElement).disabled).toBe(true);
    expect((button('停止する') as HTMLButtonElement).disabled).toBe(true);
    wrapper.unmount();
  });

  it('シェルは中身を描画せず拒否を表示する', async () => {
    stubFetch({ me: { is_admin: false } });
    const queryClient = new QueryClient();
    mount(AdminShell, {
      props: { title: 't' },
      slots: { default: '<p id="secret">secret</p>' },
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.body.textContent).toContain('管理者だけが利用できます');
    expect(document.body.querySelector('#secret')).toBeNull();
  });
});

// 本人確認の dialog のパスキーの流れ。連携の追加と初回のパスワードからの使われ方は
// AuthMethodsSection.test.ts の「本人確認」で縛る。

import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import ReauthDialog from '../ReauthDialog.vue';
import { requireRecentAuth, useReauthGate } from '@/composables/useReauthGate';

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

const STALE = {
  fresh: false,
  expires_at: null,
  remaining_secs: null,
  window_secs: 300,
  methods: { password: false, passkey: true, oauth: [] },
};

function stubFetch() {
  const finished: unknown[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => {
      const { pathname } = new URL(req.url, 'http://localhost');
      if (pathname.endsWith('/v1/auth/reauth')) return jsonResponse(STALE);
      if (pathname.endsWith('/v1/auth/reauth/passkey/start')) {
        return jsonResponse({
          publicKey: {
            challenge: 'AQID',
            allowCredentials: [{ type: 'public-key', id: 'BAUG' }],
          },
        });
      }
      if (pathname.endsWith('/v1/auth/reauth/passkey/finish')) {
        finished.push(await req.clone().json());
        return new Response(null, { status: 204 });
      }
      return jsonResponse({ message: 'not-found' }, 404);
    }),
  );
  return { finished };
}

function stubWebAuthn(get: () => Promise<unknown>) {
  vi.stubGlobal('PublicKeyCredential', class {});
  Object.defineProperty(navigator, 'credentials', { value: { get }, configurable: true });
}

const ASSERTION = {
  id: 'BAUG',
  rawId: new Uint8Array([4, 5, 6]).buffer,
  type: 'public-key',
  response: {
    authenticatorData: new Uint8Array([1]).buffer,
    clientDataJSON: new Uint8Array([2]).buffer,
    signature: new Uint8Array([3]).buffer,
    userHandle: null,
  },
};

enableAutoUnmount(afterEach);
afterEach(() => {
  useReauthGate().finish(false);
  vi.unstubAllGlobals();
  Reflect.deleteProperty(navigator, 'credentials');
});

function testid(id: string) {
  const el = document.body.querySelector<HTMLElement>(`[data-testid="${id}"]`);
  if (!el) throw new Error(`${id} not found`);
  return el;
}

describe('ReauthDialog のパスキー', () => {
  it('ブラウザの認証器で確かめると、待っておる操作へ true を返す', async () => {
    const { finished } = stubFetch();
    stubWebAuthn(async () => ASSERTION);
    mount(ReauthDialog, { props: { returnPath: '/settings/security' }, attachTo: document.body });
    const result = requireRecentAuth({ kind: 'set-password' });
    await flushPromises();

    testid('reauth-method-passkey').click();
    await flushPromises();

    await expect(result).resolves.toBe(true);
    expect(finished).toEqual([
      {
        credential: {
          id: 'BAUG',
          rawId: 'BAUG',
          type: 'public-key',
          response: {
            authenticatorData: 'AQ',
            clientDataJSON: 'Ag',
            signature: 'Aw',
            userHandle: null,
          },
          extensions: {},
        },
      },
    ]);
  });

  it('取り消すと知らせて一覧へ戻し、操作は待たせたまま', async () => {
    const { finished } = stubFetch();
    stubWebAuthn(async () => {
      throw new DOMException('cancelled', 'NotAllowedError');
    });
    mount(ReauthDialog, { props: { returnPath: '/settings/security' }, attachTo: document.body });
    let settled = false;
    void requireRecentAuth({ kind: 'set-password' }).then(() => (settled = true));
    await flushPromises();

    testid('reauth-method-passkey').click();
    await flushPromises();

    expect(testid('reauth-dialog').textContent).toContain(
      'パスキーで確かめられませんでした。もう一度試すか、ほかの方法を選んでください。',
    );
    expect(testid('reauth-method-passkey').getAttribute('aria-pressed')).toBe('false');
    expect(finished).toEqual([]);
    expect(settled).toBe(false);
  });

  it('このブラウザでパスキーを使えなければ、手段に出さない', async () => {
    stubFetch();
    vi.stubGlobal('PublicKeyCredential', undefined);
    mount(ReauthDialog, { props: { returnPath: '/settings/security' }, attachTo: document.body });
    void requireRecentAuth({ kind: 'set-password' });
    await flushPromises();

    expect(document.body.querySelector('[data-testid="reauth-method-passkey"]')).toBeNull();
    expect(testid('reauth-dialog').textContent).toContain('この画面では確かめられません');
  });
});

describe('ReauthDialog の問いの入れ替わり', () => {
  it('開いたままの dialog に次の問いが来たら、前の知らせを持ち越さない', async () => {
    stubFetch();
    vi.stubGlobal('PublicKeyCredential', undefined);
    mount(ReauthDialog, { props: { returnPath: '/settings/security' }, attachTo: document.body });
    void requireRecentAuth(
      { kind: 'set-password' },
      { notice: 'GitHub で確かめられませんでした。' },
    );
    await flushPromises();
    expect(testid('reauth-dialog').textContent).toContain('GitHub で確かめられませんでした。');

    void requireRecentAuth({ kind: 'link', provider: 'google' });
    await flushPromises();

    expect(testid('reauth-dialog').textContent).not.toContain('GitHub で確かめられませんでした。');
    expect(testid('reauth-dialog').closest('dialog')?.textContent).toContain(
      'Google の連携を追加するには本人確認が要りますが',
    );
  });
});

describe('ReauthDialog の通信の切れ', () => {
  function stubOffline(methods: { password: boolean; passkey: boolean }) {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (req: Request) => {
        const { pathname } = new URL(req.url, 'http://localhost');
        if (pathname.endsWith('/v1/auth/reauth')) {
          return jsonResponse({ ...STALE, methods: { ...methods, oauth: [] } });
        }
        // 本人確認の口へ届く前に通信が切れた
        throw new TypeError('Failed to fetch');
      }),
    );
  }

  it('パスワードで確かめる最中に切れたら、誤りを出してボタンを戻す', async () => {
    stubOffline({ password: true, passkey: false });
    mount(ReauthDialog, { props: { returnPath: '/settings/security' }, attachTo: document.body });
    let settled = false;
    void requireRecentAuth({ kind: 'set-password' }).then(() => (settled = true));
    await flushPromises();

    testid('reauth-method-password').click();
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('#reauth-password')!;
    input.value = 'Whatever123!';
    input.dispatchEvent(new Event('input'));
    await flushPromises();
    testid('reauth-password-submit').click();
    await flushPromises();

    expect(testid('reauth-dialog').closest('dialog')?.textContent).toContain(
      '通信できませんでした。接続を確かめてから、もう一度お試しください。',
    );
    expect((testid('reauth-password-submit') as HTMLButtonElement).disabled).toBe(false);
    expect((testid('reauth-method-password') as HTMLButtonElement).disabled).toBe(false);
    expect(settled).toBe(false);
  });

  it('パスキーで確かめる最中に切れたら、誤りを出して一覧へ戻す', async () => {
    stubOffline({ password: false, passkey: true });
    stubWebAuthn(async () => ASSERTION);
    mount(ReauthDialog, { props: { returnPath: '/settings/security' }, attachTo: document.body });
    void requireRecentAuth({ kind: 'set-password' });
    await flushPromises();

    testid('reauth-method-passkey').click();
    await flushPromises();

    expect(testid('reauth-dialog').textContent).toContain(
      'パスキーで確かめられませんでした。もう一度試すか、ほかの方法を選んでください。',
    );
    expect((testid('reauth-method-passkey') as HTMLButtonElement).disabled).toBe(false);
  });
});

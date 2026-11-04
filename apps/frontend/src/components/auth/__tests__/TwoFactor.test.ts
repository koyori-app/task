import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import TwoFactor from '../TwoFactor.vue';

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });

type Handler = (req: Request) => Response | Promise<Response>;

/** パスごとの応答を差し替えられる fetch。`me` の既定は半認証の 403。 */
function stubApi(handlers: Record<string, Handler>) {
  const defaults: Record<string, Handler> = {
    'GET /v1/auth/me': () => json({ message: 'forbidden' }, 403),
  };
  const all = { ...defaults, ...handlers };
  const calls: { key: string; body: unknown }[] = [];
  const fetchMock = vi.fn(async (req: Request) => {
    const key = `${req.method} ${new URL(req.url, 'http://localhost').pathname.replace(/^\/api/, '')}`;
    const text = req.method === 'GET' ? '' : await req.clone().text();
    calls.push({ key, body: text ? JSON.parse(text) : null });
    const handler = all[key];
    return handler ? handler(req) : json({ message: 'not-found' }, 404);
  });
  vi.stubGlobal('fetch', fetchMock);
  return calls;
}

const SETUP = {
  otpauth_uri: 'otpauth://totp/TaskApp:u%40example.com?secret=JBSWY3DPEHPK3PXP&issuer=TaskApp',
  qr_code_png: 'data:image/png;base64,AAAA',
};
const CODES = ['AAAA-BBBB-CCCC', 'DDDD-EEEE-FFFF'];

const verifyEnabled = {
  'POST /v1/auth/2fa/totp/setup': () => json({ message: '2fa-already-enabled' }, 409),
};
const setupNeeded = {
  'POST /v1/auth/2fa/totp/setup': () => json(SETUP),
};

async function mountPage(redirectAfter: string | null = null) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = mount(TwoFactor, {
    props: { redirectAfter },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
}

async function submitCode(wrapper: Awaited<ReturnType<typeof mountPage>>, code: string) {
  await wrapper.find('input[name="code"]').setValue(code);
  await wrapper.find('form').trigger('submit');
  await flushPromises();
}

enableAutoUnmount(afterEach);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('TwoFactor 検証（設定済み）', () => {
  it('TOTP コードを送り、成功したら /auth/me を取り直して戻り先へ進む', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const calls = stubApi({
      ...verifyEnabled,
      'POST /v1/auth/2fa/verify': () => new Response(null, { status: 204 }),
    });
    const wrapper = await mountPage('/tasks/1?x=2');

    await submitCode(wrapper, '123456');

    expect(calls.find((c) => c.key === 'POST /v1/auth/2fa/verify')?.body).toEqual({
      code: '123456',
    });
    expect(assignSpy).toHaveBeenCalledWith('/tasks/1?x=2');
  });

  it('リカバリーコードでも検証できる', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const calls = stubApi({
      ...verifyEnabled,
      'POST /v1/auth/2fa/verify': () => new Response(null, { status: 204 }),
    });
    const wrapper = await mountPage();

    await wrapper.find('button[type="button"]').trigger('click');
    await submitCode(wrapper, 'AAAA-BBBB-CCCC');

    expect(calls.find((c) => c.key === 'POST /v1/auth/2fa/verify')?.body).toEqual({
      recovery_code: 'AAAA-BBBB-CCCC',
    });
    expect(assignSpy).toHaveBeenCalledWith('/');
  });

  it('コードが違えば理由を出して遷移しない', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    stubApi({
      ...verifyEnabled,
      'POST /v1/auth/2fa/verify': () => json({ message: 'invalid-2fa-code' }, 401),
    });
    const wrapper = await mountPage();

    await submitCode(wrapper, '000000');

    expect(wrapper.text()).toContain('コードが正しくありません');
    expect(assignSpy).not.toHaveBeenCalled();
  });

  it('試行回数の上限（429）では案内を出して一時的に送信を止め、時間が経てば再送信できる', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      let status = 429;
      const calls = stubApi({
        ...verifyEnabled,
        'POST /v1/auth/2fa/verify': () =>
          status === 429
            ? json({ message: 'too-many-requests' }, 429)
            : json({ message: 'invalid-2fa-code' }, 401),
      });
      const wrapper = await mountPage();

      await submitCode(wrapper, '000000');

      expect(wrapper.text()).toContain('試行回数の上限');
      expect(wrapper.find('button[type="submit"]').attributes('disabled')).toBeDefined();

      status = 401;
      await vi.advanceTimersByTimeAsync(30_000);
      await flushPromises();

      expect(wrapper.find('button[type="submit"]').attributes('disabled')).toBeUndefined();
      await submitCode(wrapper, '111111');
      expect(calls.filter((c) => c.key === 'POST /v1/auth/2fa/verify')).toHaveLength(2);
      expect(wrapper.text()).toContain('コードが正しくありません');
    } finally {
      vi.useRealTimers();
    }
  });

  it('セッション切れ（invalid-2fa-code 以外の 401）は再サインインを案内する', async () => {
    stubApi({
      ...verifyEnabled,
      'POST /v1/auth/2fa/verify': () => json({ message: 'unauthorized' }, 401),
    });
    const wrapper = await mountPage();

    await submitCode(wrapper, '123456');

    expect(wrapper.text()).toContain('セッションの有効期限が切れました');
    expect(wrapper.text()).not.toContain('コードが正しくありません');
  });

  it('戻り先が外部 URL / //evil なら / へ進む', async () => {
    for (const evil of ['https://evil.example/', '//evil.example']) {
      const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
      stubApi({
        ...verifyEnabled,
        'POST /v1/auth/2fa/verify': () => new Response(null, { status: 204 }),
      });
      const wrapper = await mountPage(evil);

      await submitCode(wrapper, '123456');

      expect(assignSpy).toHaveBeenCalledWith('/');
      wrapper.unmount();
      assignSpy.mockRestore();
    }
  });

  it('キャンセルはサインインへ戻す', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    stubApi(verifyEnabled);
    const wrapper = await mountPage();

    const cancel = wrapper.findAll('button').find((b) => b.text().includes('キャンセル'));
    await cancel!.trigger('click');

    expect(assignSpy).toHaveBeenCalledWith('/signin');
  });
});

describe('TwoFactor 初期判定', () => {
  it('すでに完全認証（/auth/me が 200）なら入力させず戻り先へ進む', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const calls = stubApi({ 'GET /v1/auth/me': () => json({ id: 'u' }) });
    await mountPage('/foo');

    expect(assignSpy).toHaveBeenCalledWith('/foo');
    expect(calls.some((c) => c.key.includes('/2fa/'))).toBe(false);
  });

  it('未ログイン（401）なら再サインインを案内する', async () => {
    stubApi({ 'GET /v1/auth/me': () => json({ message: 'unauthorized' }, 401) });
    const wrapper = await mountPage();

    expect(wrapper.text()).toContain('セッションの有効期限が切れました');
  });
});

describe('TwoFactor セットアップ（未設定・強制）', () => {
  it('QR と手入力用シークレットを出し、初回コードの検証後にリカバリーコードを見せてから進む', async () => {
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const calls = stubApi({
      ...setupNeeded,
      'POST /v1/auth/2fa/totp/verify-setup': () => json({ recovery_codes: CODES }),
    });
    const wrapper = await mountPage('/foo');

    expect(wrapper.find('img').attributes('src')).toBe(SETUP.qr_code_png);
    expect(wrapper.find('[data-testid="totp-secret"]').text()).toBe('JBSWY3DPEHPK3PXP');
    expect(wrapper.find('[data-testid="totp-uri"]').text()).toBe(SETUP.otpauth_uri);

    await submitCode(wrapper, '123456');

    expect(calls.find((c) => c.key === 'POST /v1/auth/2fa/totp/verify-setup')?.body).toEqual({
      code: '123456',
    });
    // リカバリーコードを見せている間は先へ進まない
    expect(wrapper.find('[data-testid="recovery-codes"]').text()).toContain('AAAA-BBBB-CCCC');
    expect(assignSpy).not.toHaveBeenCalled();
    const next = wrapper.findAll('button').find((b) => b.text() === '続ける')!;
    expect(next.attributes('disabled')).toBeDefined();

    // 保存の確認を入れて初めて進める
    await wrapper.find('input[type="checkbox"]').setValue(true);
    await flushPromises();
    await next.trigger('click');
    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith('/foo');
  });

  it('リカバリーコードは再表示されない（進んだ後・開き直した後に出ない）', async () => {
    vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    stubApi({
      ...setupNeeded,
      'POST /v1/auth/2fa/totp/verify-setup': () => json({ recovery_codes: CODES }),
    });
    const wrapper = await mountPage();
    await submitCode(wrapper, '123456');
    await wrapper.find('input[type="checkbox"]').setValue(true);
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((b) => b.text() === '続ける')!
      .trigger('click');
    await flushPromises();

    expect(wrapper.text()).not.toContain('AAAA-BBBB-CCCC');
    wrapper.unmount();

    // 画面を開き直すと、設定済みなので検証の画面になり、コードは出ない
    stubApi(verifyEnabled);
    const reopened = await mountPage();
    expect(reopened.text()).not.toContain('AAAA-BBBB-CCCC');
    expect(reopened.find('[data-testid="recovery-codes"]').exists()).toBe(false);
  });

  it('初回コードが違えば理由を出し、リカバリーコードは出さない', async () => {
    stubApi({
      ...setupNeeded,
      'POST /v1/auth/2fa/totp/verify-setup': () => json({ message: 'invalid-2fa-code' }, 401),
    });
    const wrapper = await mountPage();

    await submitCode(wrapper, '000000');

    expect(wrapper.text()).toContain('コードが正しくありません');
    expect(wrapper.find('[data-testid="recovery-codes"]').exists()).toBe(false);
  });
});

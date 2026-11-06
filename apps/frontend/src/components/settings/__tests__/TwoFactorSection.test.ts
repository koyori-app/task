// - 有効化（QR → 初回コード → リカバリーコード表示 → /me 再取得）
// - 無効化（TOTP / リカバリーコード）、再生成（TOTP のみ）
// - 不正コード・使用済みリカバリーコード（401）、ロックアウト（429）、強制テナント（403）
// - 失敗時に状態表示を切り替えないこと、リカバリーコードを永続化しないこと
import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import TwoFactorSection from '../TwoFactorSection.vue';
import { meQueryOptions } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });

type Handler = (req: Request) => Response | Promise<Response>;

function stubApi(handlers: Record<string, Handler>) {
  const calls: { key: string; body: unknown }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => {
      const key = `${req.method} ${new URL(req.url, 'http://localhost').pathname.replace(/^\/api/, '')}`;
      const text = req.method === 'GET' ? '' : await req.clone().text();
      calls.push({ key, body: text ? JSON.parse(text) : null });
      const h = handlers[key];
      return h ? h(req) : json({ message: 'not-found' }, 404);
    }),
  );
  return calls;
}

const SETUP = {
  otpauth_uri: 'otpauth://totp/TaskApp:u%40example.com?secret=JBSWY3DPEHPK3PXP&issuer=TaskApp',
  qr_code_png: 'data:image/png;base64,AAAA',
};
const CODES = ['AAAA-BBBB-CCCC', 'DDDD-EEEE-FFFF'];
const ME = 'GET /v1/auth/me';
/** 初回の取得（1 回）を除いた再取得の回数。 */
const meCalls = (calls: { key: string }[]) => calls.filter((c) => c.key === ME).length - 1;

function user(totp: boolean): components['schemas']['UserResponse'] {
  return {
    id: '00000000-0000-0000-0000-000000000001',
    username: 'tester',
    email: 'tester@example.com',
    email_verified: true,
    is_admin: false,
    is_suspended: false,
    totp_enabled: totp,
    has_password: true,
    bio: null,
    avatar_url: null,
  };
}

/** 実画面では親が /me を購読している。キャッシュに載せて、再取得の回数を数える。 */
function mountSection(totp: boolean) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  void queryClient.prefetchQuery(meQueryOptions());
  return mount(TwoFactorSection, {
    props: { user: user(totp) },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

async function click(w: ReturnType<typeof mountSection>, label: string) {
  const b = w.findAll('button').find((x) => x.text().includes(label));
  expect(b, label).toBeTruthy();
  await b!.trigger('click');
  await flushPromises();
}

async function submitCode(w: ReturnType<typeof mountSection>, code: string) {
  await w.find('input[name="code"]').setValue(code);
  await w.find('form').trigger('submit');
  await flushPromises();
}

enableAutoUnmount(afterEach);
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('TwoFactorSection 有効化', () => {
  it('QR → 初回コード → リカバリーコード表示 → 完了で消え、/me を取り直す', async () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    const calls = stubApi({
      'POST /v1/auth/2fa/totp/setup': () => json(SETUP),
      'POST /v1/auth/2fa/totp/verify-setup': () => json({ recovery_codes: CODES }),
      [ME]: () => json(user(true)),
    });
    const w = mountSection(false);
    await click(w, '有効にする');
    expect(w.find('img').attributes('src')).toBe(SETUP.qr_code_png);
    expect(w.find('[data-testid="totp-secret"]').text()).toBe('JBSWY3DPEHPK3PXP');

    await submitCode(w, '123456');
    expect(calls.find((c) => c.key === 'POST /v1/auth/2fa/totp/verify-setup')?.body).toEqual({
      code: '123456',
    });
    expect(w.find('[data-testid="recovery-codes"]').text()).toContain(CODES[0]);
    expect(meCalls(calls)).toBe(1);
    expect(setItem).not.toHaveBeenCalled();

    await w.find('input[type="checkbox"], [role="checkbox"]').trigger('click');
    await click(w, '続ける');
    expect(w.text()).not.toContain(CODES[0]);
  });

  it('不正なコードは誤り表示のみで、リカバリーコードも状態変更も出さない', async () => {
    const calls = stubApi({
      'POST /v1/auth/2fa/totp/setup': () => json(SETUP),
      'POST /v1/auth/2fa/totp/verify-setup': () => json({ message: 'invalid-2fa-code' }, 401),
    });
    const w = mountSection(false);
    await click(w, '有効にする');
    await submitCode(w, '000000');
    expect(w.text()).toContain('コードが正しくありません');
    expect(w.find('[data-testid="recovery-codes"]').exists()).toBe(false);
    expect(meCalls(calls)).toBe(0);
  });

  it('429 でロックアウトし送信を止める', async () => {
    stubApi({
      'POST /v1/auth/2fa/totp/setup': () => json(SETUP),
      'POST /v1/auth/2fa/totp/verify-setup': () => json({ message: 'too-many' }, 429),
    });
    const w = mountSection(false);
    await click(w, '有効にする');
    await submitCode(w, '000000');
    expect(w.text()).toContain('試行回数の上限');
    expect(w.find('button[type="submit"]').attributes('disabled')).toBeDefined();
  });
});

describe('TwoFactorSection 無効化', () => {
  it('TOTP で無効化し /me を取り直す', async () => {
    const calls = stubApi({
      'DELETE /v1/auth/2fa/totp': () => new Response(null, { status: 204 }),
      [ME]: () => json(user(false)),
    });
    const w = mountSection(true);
    await click(w, '無効にする');
    await submitCode(w, '123456');
    expect(calls.find((c) => c.key === 'DELETE /v1/auth/2fa/totp')?.body).toEqual({
      code: '123456',
    });
    expect(meCalls(calls)).toBe(1);
  });

  it('リカバリーコードでも無効化できる', async () => {
    const calls = stubApi({
      'DELETE /v1/auth/2fa/totp': () => new Response(null, { status: 204 }),
      [ME]: () => json(user(false)),
    });
    const w = mountSection(true);
    await click(w, '無効にする');
    await click(w, 'リカバリーコードを使う');
    await submitCode(w, 'AAAA-BBBB-CCCC');
    expect(calls.find((c) => c.key === 'DELETE /v1/auth/2fa/totp')?.body).toEqual({
      recovery_code: 'AAAA-BBBB-CCCC',
    });
  });

  it('使用済みリカバリーコード（401）は誤り表示で、無効化済みに見せない', async () => {
    const calls = stubApi({
      'DELETE /v1/auth/2fa/totp': () => json({ message: 'invalid-2fa-code' }, 401),
    });
    const w = mountSection(true);
    await click(w, '無効にする');
    await click(w, 'リカバリーコードを使う');
    await submitCode(w, 'USED-USED-USED');
    expect(w.text()).toContain('コードが正しくありません');
    expect(w.text()).toContain('有効です。');
    expect(meCalls(calls)).toBe(0);
  });

  it('強制テナント（403）は理由を表示し、状態を変えない', async () => {
    const calls = stubApi({
      'DELETE /v1/auth/2fa/totp': () => json({ message: 'forbidden' }, 403),
    });
    const w = mountSection(true);
    await click(w, '無効にする');
    await submitCode(w, '123456');
    expect(w.text()).toContain('必須にしているため、無効化できません');
    expect(w.text()).toContain('有効です。');
    expect(meCalls(calls)).toBe(0);
  });
});

describe('TwoFactorSection 再生成', () => {
  it('TOTP のみ受け付け、新しいコードを表示して /me を取り直さずに閉じる', async () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    const calls = stubApi({
      'POST /v1/auth/2fa/recovery-codes/regenerate': () => json({ recovery_codes: CODES }),
    });
    const w = mountSection(true);
    await click(w, 'リカバリーコードを再生成');
    expect(w.text()).not.toContain('リカバリーコードを使う');
    await submitCode(w, '123456');
    expect(calls.find((c) => c.key.includes('regenerate'))?.body).toEqual({ code: '123456' });
    expect(w.find('[data-testid="recovery-codes"]').text()).toContain(CODES[1]);
    expect(setItem).not.toHaveBeenCalled();
  });

  it('不正な TOTP ではコードを表示しない', async () => {
    stubApi({
      'POST /v1/auth/2fa/recovery-codes/regenerate': () =>
        json({ message: 'invalid-2fa-code' }, 401),
    });
    const w = mountSection(true);
    await click(w, 'リカバリーコードを再生成');
    await submitCode(w, '000000');
    expect(w.text()).toContain('コードが正しくありません');
    expect(w.find('[data-testid="recovery-codes"]').exists()).toBe(false);
  });
});

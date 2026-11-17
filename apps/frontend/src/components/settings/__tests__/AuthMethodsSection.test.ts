// テストケースの一覧（docs/dev/features/account-settings.md から移した）
// - パスワードあり／なしの出し分け、初回設定に現在のパスワード欄を出さないこと、確認不一致で送信しないこと
// - 現在のパスワード不一致の表示、変更後のサインイン画面への遷移
// - 連携一覧と追加候補、self-hosted の URL 未入力での無効化、連携済みでも別インスタンスを開始できること、同一インスタンスを開始しないこと
// - 成功通知がマーカーを根拠にすること（URL だけでは出ないこと）
// - 確認を挟む解除と `instance_url` の付与、最後の認証方法の拒否表示
// - 本人確認: 連携の追加と初回のパスワードの前に問い、要れば dialog で確かめてから押し直さずに続けること、
//   連携で確かめて戻った後に続けること、確かめられずに戻った時に出し直すこと、GitHub App は問わないこと

import { describe, it, expect, afterEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import AuthMethodsSection from '../AuthMethodsSection.vue';
import { useReauthGate } from '@/composables/useReauthGate';
import { OAUTH_LINK_NOTICE, REAUTH_RESUME_NOTICE } from '@/lib/one-time-notice';
import type { components } from '@/generated/api';

const USER_ID = '00000000-0000-0000-0000-000000000001';
const INSTANCE_URL = 'https://gitlab.example.com';
/** 汎用 OIDC の連携は issuer 込みで保存される（backend の `db_provider_key`）。 */
const OIDC_CONNECTION = 'oidc:https://idp.example.com';

function user(hasPassword: boolean): components['schemas']['UserResponse'] {
  return {
    id: USER_ID,
    username: 'tester',
    email: 'tester@example.com',
    email_verified: true,
    is_admin: false,
    is_suspended: false,
    totp_enabled: false,
    has_password: hasPassword,
    bio: null,
    avatar_url: null,
  };
}

type Connection = components['schemas']['OAuthConnectionItem'];
type Provider = components['schemas']['OAuthProviderItem'];

type MockState = {
  connections: Connection[];
  providers: Provider[];
  passkeyCount?: number;
  /** DELETE /oauth/connections/{provider} の返り値。未指定なら 204。 */
  disconnectError?: { status: number; message: string };
  /** POST /password と /password/change の返り値。未指定なら成功。 */
  passwordError?: { status: number; message: string };
  /** POST /password が先に何度 reauth-required を返すか（打っている間に期間が切れた形）。 */
  passwordReauthRequired?: number;
  /** GET /v1/auth/reauth の返り値。未指定なら印あり（関所を素通りする）。 */
  reauth?: ReauthStatus | { status: number };
  /** POST /v1/auth/reauth/password の正しいパスワード。合えば印を立てる。 */
  reauthPassword?: string;
};

type ReauthStatus = components['schemas']['ReauthStatusResponse'];

const FRESH: ReauthStatus = {
  fresh: true,
  expires_at: '2999-01-01T00:00:00Z',
  window_secs: 300,
  methods: { password: true, passkey: false, oauth: [] },
};

function stale(methods: Partial<ReauthStatus['methods']> = {}): ReauthStatus {
  return {
    fresh: false,
    expires_at: null,
    window_secs: 300,
    methods: { password: true, passkey: false, oauth: [], ...methods },
  };
}

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

function stubFetch(state: MockState) {
  const deleted: string[] = [];
  const passwordBodies: unknown[] = [];
  const reauthQueries: number[] = [];
  let reauthRequired = state.passwordReauthRequired ?? 0;

  const fetchMock = vi.fn(async (req: Request | string, init?: RequestInit) => {
    const rawUrl = typeof req === 'string' ? req : req.url;
    const method = typeof req === 'string' ? (init?.method ?? 'GET') : req.method;
    const url = new URL(rawUrl, 'http://localhost');
    const pathname = url.pathname;

    // 強度判定はサーバー側 API。debounce 越しに飛んでも落ちないようにしておく。
    if (pathname.endsWith('/internal/password-strength')) {
      return jsonResponse({ strength: 'high' });
    }
    if (method === 'GET' && pathname.endsWith('/v1/auth/oauth/connections')) {
      return jsonResponse({ connections: state.connections });
    }
    if (method === 'GET' && pathname.endsWith('/v1/auth/oauth/providers')) {
      return jsonResponse({ providers: state.providers });
    }
    if (method === 'GET' && pathname.endsWith('/v1/auth/passkeys')) {
      return jsonResponse({
        passkeys: Array.from({ length: state.passkeyCount ?? 0 }, (_, i) => ({
          id: `passkey-${i}`,
          name: `key ${i}`,
          created_at: '2026-09-01T00:00:00Z',
          last_used_at: null,
        })),
      });
    }
    if (method === 'GET' && pathname.endsWith('/v1/auth/reauth')) {
      reauthQueries.push(1);
      const reauth = state.reauth ?? FRESH;
      if ('status' in reauth) return jsonResponse({ message: 'internal' }, reauth.status);
      return jsonResponse(reauth);
    }
    if (method === 'POST' && pathname.endsWith('/v1/auth/reauth/password')) {
      const body = (await (req as Request).clone().json()) as { password: string };
      if (body.password !== state.reauthPassword) {
        return jsonResponse({ message: 'invalid-current-password' }, 400);
      }
      state.reauth = FRESH;
      return new Response(null, { status: 204 });
    }
    if (method === 'DELETE' && pathname.includes('/v1/auth/oauth/connections/')) {
      deleted.push(`${pathname}${url.search}`);
      if (state.disconnectError) {
        return jsonResponse(
          { message: state.disconnectError.message },
          state.disconnectError.status,
        );
      }
      const provider = pathname.split('/').pop();
      state.connections = state.connections.filter((c) => c.provider !== provider);
      return new Response(null, { status: 204 });
    }
    if (method === 'POST' && pathname.endsWith('/v1/auth/password')) {
      passwordBodies.push(typeof req === 'string' ? init?.body : await req.clone().json());
      if (reauthRequired > 0) {
        reauthRequired -= 1;
        state.reauth = stale();
        return jsonResponse({ message: 'reauth-required' }, 403);
      }
      if (state.passwordError) {
        return jsonResponse({ message: state.passwordError.message }, state.passwordError.status);
      }
      return new Response(null, { status: 204 });
    }
    if (method === 'POST' && pathname.endsWith('/v1/auth/password/change')) {
      passwordBodies.push(typeof req === 'string' ? init?.body : await req.clone().json());
      if (state.passwordError) {
        return jsonResponse({ message: state.passwordError.message }, state.passwordError.status);
      }
      return jsonResponse({ message: 'ok' });
    }
    return jsonResponse({ message: 'not-found' }, 404);
  });

  vi.stubGlobal('fetch', fetchMock);
  return { deleted, passwordBodies, reauthQueries };
}

function mountSection(hasPassword = true) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(AuthMethodsSection, {
    props: { user: user(hasPassword) },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function bodyButton(label: string) {
  return [...document.body.querySelectorAll('button')].find((b) => b.textContent?.trim() === label);
}

function clickBodyButton(label: string) {
  const button = bodyButton(label);
  if (!button) throw new Error(`button "${label}" not found`);
  button.click();
}

/** `/oauth/providers` の 1 件。OIDC だけ連携一覧の識別子が slug と違う。 */
function provider(slug: string, connectionProvider = slug): Provider {
  return {
    provider: slug,
    connection_provider: connectionProvider,
    requires_instance_url: slug === 'gitlab_selfhosted',
    sign_in: slug !== 'github_app',
  };
}

function connection(overrides: Partial<Connection> = {}): Connection {
  return {
    provider: 'github',
    provider_email: 'tester@example.com',
    instance_url: null,
    connected_at: '2026-09-01T10:00:00Z',
    sign_in: true,
    ...overrides,
  };
}

enableAutoUnmount(afterEach);

afterEach(() => {
  // 本人確認の関所は画面をまたいで一つなので、開いたまま次の試験へ持ち越さない
  useReauthGate().finish(false);
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  // 通知の印はタブに残るので、テスト間で持ち越さない
  window.sessionStorage.clear();
});

describe('AuthMethodsSection のパスワード', () => {
  it('設定済みならメールアドレスと変更ボタンを出す', async () => {
    stubFetch({ connections: [], providers: [] });
    mountSection(true);
    await flushPromises();

    expect(document.body.textContent).toContain('設定済み');
    expect(document.body.textContent).toContain('tester@example.com');
    expect(bodyButton('パスワードを変更')).toBeTruthy();
    expect(bodyButton('パスワードを設定')).toBeUndefined();
  });

  it('未設定なら OAuth のみである旨と設定ボタンを出す', async () => {
    stubFetch({ connections: [connection()], providers: [] });
    mountSection(false);
    await flushPromises();

    expect(document.body.textContent).toContain('未設定');
    expect(document.body.textContent).toContain('OAuth 連携のみでサインインしています');
    expect(bodyButton('パスワードを設定')).toBeTruthy();
  });

  it('初回設定では現在のパスワード欄を出さず、設定すると通知を出す', async () => {
    const { passwordBodies } = stubFetch({ connections: [connection()], providers: [] });
    const wrapper = mountSection(false);
    await flushPromises();

    clickBodyButton('パスワードを設定');
    await flushPromises();
    expect(document.body.querySelector('#password-current')).toBeNull();

    await wrapper.find('#password-next').setValue('NewPassword123');
    await wrapper.find('#password-confirm').setValue('NewPassword123');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(passwordBodies).toEqual([{ password: 'NewPassword123' }]);
    expect(document.body.textContent).toContain('パスワードを設定しました。');
  });

  it('確認が一致しなければ送信しない', async () => {
    const { passwordBodies } = stubFetch({ connections: [], providers: [] });
    const wrapper = mountSection(true);
    await flushPromises();

    clickBodyButton('パスワードを変更');
    await flushPromises();

    await wrapper.find('#password-current').setValue('OldPassword123');
    await wrapper.find('#password-next').setValue('NewPassword123');
    await wrapper.find('#password-confirm').setValue('NewPassword124');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(passwordBodies).toEqual([]);
    expect(document.body.textContent).toContain('パスワードが一致しません。');
  });

  it('現在のパスワードが違えばサーバーの拒否を伝える', async () => {
    stubFetch({
      connections: [],
      providers: [],
      passwordError: { status: 400, message: 'invalid-current-password' },
    });
    const wrapper = mountSection(true);
    await flushPromises();

    clickBodyButton('パスワードを変更');
    await flushPromises();

    await wrapper.find('#password-current').setValue('WrongPassword123');
    await wrapper.find('#password-next').setValue('NewPassword123');
    await wrapper.find('#password-confirm').setValue('NewPassword123');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(document.body.textContent).toContain('現在のパスワードが違います。');
  });

  /** 変更でセッションと PAT が失効するので、そのままの状態で留まらせない。 */
  it('変更に成功したらサインイン画面へフルページ遷移する', async () => {
    stubFetch({ connections: [], providers: [] });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const wrapper = mountSection(true);
    await flushPromises();

    clickBodyButton('パスワードを変更');
    await flushPromises();

    await wrapper.find('#password-current').setValue('OldPassword123');
    await wrapper.find('#password-next').setValue('NewPassword123');
    await wrapper.find('#password-confirm').setValue('NewPassword123');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith('/signin');
  });
});

describe('AuthMethodsSection の OAuth 連携', () => {
  it('連携済みプロバイダーの内訳を出す', async () => {
    stubFetch({
      connections: [
        connection({
          provider: 'gitlab_selfhosted',
          provider_email: 'dev@example.com',
          instance_url: INSTANCE_URL,
        }),
      ],
      providers: [provider('gitlab_selfhosted')],
    });
    mountSection(true);
    await flushPromises();

    expect(document.body.textContent).toContain('GitLab (セルフホスト)');
    expect(document.body.textContent).toContain('dev@example.com');
    expect(document.body.textContent).toContain('接続日時');
    expect(document.body.textContent).toContain(INSTANCE_URL);
    // インスタンスが違えば別の連携なので、1 件連携済みでも追加の口は残す
    expect(document.body.textContent).toContain('追加できる連携');
  });

  it('self-hosted は連携済みでも別のインスタンスを開始できる', async () => {
    stubFetch({
      connections: [connection({ provider: 'gitlab_selfhosted', instance_url: INSTANCE_URL })],
      providers: [provider('gitlab_selfhosted')],
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const wrapper = mountSection(true);
    await flushPromises();

    const other = 'https://gitlab.internal.example.com';
    await wrapper.find('#instance-gitlab_selfhosted').setValue(other);
    await flushPromises();

    expect((bodyButton('連携する') as HTMLButtonElement).disabled).toBe(false);
    clickBodyButton('連携する');
    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith(
      `/api/v1/auth/oauth/gitlab_selfhosted?redirect_after=%2Fsettings%2Fsecurity&error_redirect_after=%2Fsettings%2Fsecurity&instance_url=${encodeURIComponent(other)}`,
    );
  });

  it('連携済みと同じインスタンスは開始しない', async () => {
    stubFetch({
      connections: [connection({ provider: 'gitlab_selfhosted', instance_url: INSTANCE_URL })],
      providers: [provider('gitlab_selfhosted')],
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const wrapper = mountSection(true);
    await flushPromises();

    // 末尾のスラッシュ違いは同じインスタンスとして扱う
    await wrapper.find('#instance-gitlab_selfhosted').setValue(`${INSTANCE_URL}/`);
    await flushPromises();

    const button = bodyButton('連携する') as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    button.click();
    expect(assignSpy).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('このインスタンスは連携済みです');
  });

  // 重複の判定は backend も保存した文字列の完全一致。画面が origin まで正規化して
  // ホスト名の大小を同一視すると、backend では足せるインスタンスを止めてしまう
  it('ホスト名の大小が違うインスタンスは別の連携として開始できる', async () => {
    stubFetch({
      connections: [
        connection({ provider: 'gitlab_selfhosted', instance_url: 'https://GitLab.example.com' }),
      ],
      providers: [provider('gitlab_selfhosted')],
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const wrapper = mountSection(true);
    await flushPromises();

    await wrapper.find('#instance-gitlab_selfhosted').setValue(INSTANCE_URL);
    await flushPromises();

    expect(document.body.textContent).not.toContain('このインスタンスは連携済みです');
    expect((bodyButton('連携する') as HTMLButtonElement).disabled).toBe(false);
    clickBodyButton('連携する');
    await flushPromises();
    expect(assignSpy).toHaveBeenCalledWith(
      `/api/v1/auth/oauth/gitlab_selfhosted?redirect_after=%2Fsettings%2Fsecurity&error_redirect_after=%2Fsettings%2Fsecurity&instance_url=${encodeURIComponent(INSTANCE_URL)}`,
    );
  });

  it('未連携のプロバイダーだけ追加候補に出す', async () => {
    stubFetch({
      connections: [connection({ provider: 'github' })],
      providers: [provider('github'), provider('google')],
    });
    mountSection(true);
    await flushPromises();

    expect(document.body.textContent).toContain('追加できる連携');
    expect(document.body.textContent).toContain('Google');
  });

  it('self-hosted はインスタンス URL 未入力だと連携ボタンを押せない', async () => {
    stubFetch({
      connections: [],
      providers: [provider('gitlab_selfhosted')],
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const wrapper = mountSection(true);
    await flushPromises();

    const button = bodyButton('連携する') as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    button.click();
    expect(assignSpy).not.toHaveBeenCalled();

    await wrapper.find('#instance-gitlab_selfhosted').setValue(INSTANCE_URL);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();
    expect(assignSpy).toHaveBeenCalledWith(
      `/api/v1/auth/oauth/gitlab_selfhosted?redirect_after=%2Fsettings%2Fsecurity&error_redirect_after=%2Fsettings%2Fsecurity&instance_url=${encodeURIComponent(INSTANCE_URL)}`,
    );
  });

  it('確認してから解除し、instance_url を添えて送る', async () => {
    const { deleted } = stubFetch({
      connections: [
        connection({ provider: 'gitlab_selfhosted', instance_url: INSTANCE_URL }),
        connection({ provider: 'github' }),
      ],
      providers: [provider('gitlab_selfhosted'), provider('github')],
    });
    mountSection(true);
    await flushPromises();

    // 確認を挟むまでは何も送らない
    [...document.body.querySelectorAll('button')]
      .filter((b) => b.textContent?.trim() === '解除')[0]
      .click();
    await flushPromises();
    expect(deleted).toEqual([]);

    clickBodyButton('解除する');
    await flushPromises();

    expect(deleted).toEqual([
      `/api/v1/auth/oauth/connections/gitlab_selfhosted?instance_url=${encodeURIComponent(INSTANCE_URL)}`,
    ]);
    expect(document.body.textContent).toContain('GitLab (セルフホスト) の連携を解除しました。');
  });

  it('最後の認証方法はサーバーの拒否をそのまま伝える', async () => {
    stubFetch({
      connections: [connection({ provider: 'github' })],
      providers: [provider('github')],
      disconnectError: { status: 403, message: 'oauth-last-auth-method' },
    });
    mountSection(false);
    await flushPromises();

    clickBodyButton('解除');
    await flushPromises();
    clickBodyButton('解除する');
    await flushPromises();

    expect(document.body.textContent).toContain('これが最後の認証方法のため解除できません。');
  });

  it('認証方法が1つだけなら解除前に注意を出す', async () => {
    stubFetch({
      connections: [connection({ provider: 'github' })],
      providers: [provider('github')],
    });
    mountSection(false);
    await flushPromises();

    expect(document.body.textContent).toContain('これが最後の認証方法の可能性があります。');
  });

  // backend は OIDC の連携を `oidc:{issuer}` で保存する。開始用 slug の `oidc` で
  // 突き合わせると、連携済みでも候補に残り続ける
  it('OIDC は連携済みなら追加候補に出さない', async () => {
    stubFetch({
      connections: [connection({ provider: OIDC_CONNECTION })],
      providers: [provider('oidc', OIDC_CONNECTION)],
    });
    mountSection(true);
    await flushPromises();

    // issuer 付きの生の識別子ではなく表示名で出る
    expect(document.body.textContent).toContain('OIDC');
    expect(document.body.textContent).not.toContain('追加できる連携');
  });

  /** PR 承認用の GitHub 連携はサインインに使えないので、認証方法に数えない。 */
  it('PR 承認用の連携があっても、ログイン用の連携が 1 つなら注意を出す', async () => {
    stubFetch({
      connections: [
        connection({ provider: 'github' }),
        connection({ provider: 'github_app', provider_email: null, sign_in: false }),
      ],
      providers: [provider('github'), provider('github_app')],
    });
    mountSection(false);
    await flushPromises();

    expect(document.body.textContent).toContain('GitHub（PR 承認用）');
    // 注意はログイン用の連携の行にだけ出る（承認用の連携を解除してもサインインは困らない）
    expect(
      document.body.textContent?.split('これが最後の認証方法の可能性があります。').length,
    ).toBe(2);
  });

  /** 連携専用かは連携ごとの値で決める。プロバイダー一覧から引くと、設定が外れた後に数え直してしまう。 */
  it('GitHub App の設定が外れても、残った PR 承認用の連携を認証方法に数えない', async () => {
    stubFetch({
      connections: [
        connection({ provider: 'github' }),
        connection({ provider: 'github_app', provider_email: null, sign_in: false }),
      ],
      providers: [provider('github')],
    });
    mountSection(false);
    await flushPromises();

    expect(
      document.body.textContent?.split('これが最後の認証方法の可能性があります。').length,
    ).toBe(2);
  });

  /** パスキーも認証方法。数え落とすと最後でないのに注意が出る。 */
  it('パスキーがあれば最後の認証方法の注意を出さない', async () => {
    stubFetch({
      connections: [connection({ provider: 'github' })],
      providers: [provider('github')],
      passkeyCount: 1,
    });
    mountSection(false);
    await flushPromises();

    expect(document.body.textContent).not.toContain('これが最後の認証方法の可能性があります。');
  });
});

describe('AuthMethodsSection の成功通知', () => {
  /** 連携から戻った直後の URL を作る。history.state も載せて引き継ぎを見る。 */
  function enterWith(search: string, state: unknown = { vike: 'routed' }) {
    window.history.replaceState(state, '', `/settings/security${search}`);
  }

  /** 「連携する」を押した側が置く印。連携一覧で突き合わせる識別子まで含む。 */
  function markStarted(slug: string, connectionProvider = slug, instanceUrl = '') {
    window.sessionStorage.setItem(
      OAUTH_LINK_NOTICE,
      JSON.stringify({ provider: slug, connectionProvider, instanceUrl }),
    );
  }

  it('自分で始めた連携が一覧に入っていれば通知を出す', async () => {
    stubFetch({ connections: [connection()], providers: [] });
    markStarted('github');
    enterWith('');
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).toContain('GitHub を連携しました。');
  });

  // URL を開かせるだけで正規の成功通知を出せると、連携していない人に
  // 連携できたと思わせられる。通知の根拠は本人のタブに置いた印だけにする
  it('印が無ければ URL に linked を付けても通知を出さない', async () => {
    stubFetch({ connections: [connection()], providers: [] });
    enterWith('?linked=github');
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).not.toContain('を連携しました。');
  });

  // 承認の途中で失敗するとこの画面には戻らない。印だけを根拠にすると、
  // 次にここを開いた時点で連携できたことになってしまう
  it('印があっても一覧に入っていなければ通知を出さない', async () => {
    stubFetch({ connections: [], providers: [] });
    markStarted('github');
    enterWith('');
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).not.toContain('を連携しました。');
  });

  // 開始用 slug は `oidc` だが、連携一覧は `oidc:{issuer}` を返す。slug で突き合わせると
  // 連携できていても通知が出ない
  it('OIDC は issuer 付きの識別子で一覧と突き合わせる', async () => {
    stubFetch({ connections: [connection({ provider: OIDC_CONNECTION })], providers: [] });
    markStarted('oidc', OIDC_CONNECTION);
    enterWith('');
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).toContain('OIDC を連携しました。');
  });

  // インスタンス A を連携済みのまま B の承認を中断して開き直すと、B は連携していない。
  // プロバイダー名だけで突き合わせると A を見て成功と判定してしまう
  it('別のインスタンスが連携済みでも、開始した方が入るまで通知しない', async () => {
    stubFetch({
      connections: [connection({ provider: 'gitlab_selfhosted', instance_url: INSTANCE_URL })],
      providers: [],
    });
    markStarted('gitlab_selfhosted', 'gitlab_selfhosted', 'https://gitlab.internal.example.com');
    enterWith('');
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).not.toContain('を連携しました。');
  });

  it('開始したインスタンスが一覧に入っていれば通知を出す', async () => {
    stubFetch({
      connections: [connection({ provider: 'gitlab_selfhosted', instance_url: INSTANCE_URL })],
      providers: [],
    });
    // 末尾のスラッシュ違いは同じインスタンスとして扱う
    markStarted('gitlab_selfhosted', 'gitlab_selfhosted', `${INSTANCE_URL}/`);
    enterWith('');
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).toContain('GitLab (セルフホスト) を連携しました。');
  });

  it('印は一度きりで、開き直しても通知は出ない', async () => {
    stubFetch({ connections: [connection()], providers: [] });
    markStarted('github');
    enterWith('');
    mountSection();
    await flushPromises();

    const second = mountSection();
    await flushPromises();

    expect(second.text()).not.toContain('を連携しました。');
  });

  it('プロバイダー側の失敗は通知に変えない', async () => {
    stubFetch({ connections: [connection()], providers: [] });
    markStarted('github');
    enterWith('?oauth_error=access_denied');
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).not.toContain('を連携しました。');
    expect(wrapper.text()).toContain('外部プロバイダーでの連携に失敗しました。');
    expect(window.location.search).toBe('');
  });

  it('失敗の印を落とすときに history.state を捨てない', async () => {
    stubFetch({ connections: [connection()], providers: [] });
    enterWith('?oauth_error=access_denied', { vike: 'routed' });
    mountSection();
    await flushPromises();

    expect(window.history.state).toEqual({ vike: 'routed' });
  });
});

describe('AuthMethodsSection の本人確認', () => {
  /** 題と説明は Dialog の見出しにあるので、dialog 全体を見る。 */
  function dialog() {
    return document.body.querySelector('[data-testid="reauth-dialog"]')?.closest('dialog') ?? null;
  }
  function testid(id: string) {
    const el = document.body.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    if (!el) throw new Error(`${id} not found`);
    return el;
  }
  async function typeReauthPassword(value: string) {
    testid('reauth-method-password').click();
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('#reauth-password');
    if (!input) throw new Error('reauth password input not found');
    input.value = value;
    input.dispatchEvent(new Event('input'));
    await flushPromises();
  }
  const GOOGLE_LINK =
    '/api/v1/auth/oauth/google?redirect_after=%2Fsettings%2Fsecurity&error_redirect_after=%2Fsettings%2Fsecurity';

  it('印が無ければ連携の画面へ移らずに確かめさせ、確かめると押し直さずに移る', async () => {
    stubFetch({
      connections: [connection()],
      providers: [provider('google')],
      reauth: stale(),
      reauthPassword: 'Correct123!',
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();
    expect(assignSpy).not.toHaveBeenCalled();
    expect(dialog()?.textContent).toContain(
      'Google の連携を追加する前に、本人であることを確かめます。',
    );
    expect(dialog()?.textContent).toContain('5 分間');

    await typeReauthPassword('Correct123!');
    testid('reauth-password-submit').click();
    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith(GOOGLE_LINK);
  });

  it('パスワードが違えば欄の下に出し、移らない', async () => {
    stubFetch({
      connections: [connection()],
      providers: [provider('google')],
      reauth: stale(),
      reauthPassword: 'Correct123!',
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();
    await typeReauthPassword('Wrong123!');
    testid('reauth-password-submit').click();
    await flushPromises();

    expect(dialog()?.textContent).toContain('パスワードが違います。もう一度入力してください。');
    expect(assignSpy).not.toHaveBeenCalled();
  });

  it('取り消すと移らない', async () => {
    stubFetch({ connections: [connection()], providers: [provider('google')], reauth: stale() });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();
    const cancel = [...(dialog()?.querySelectorAll('button') ?? [])].find(
      (b) => b.textContent?.trim() === 'キャンセル',
    );
    cancel?.click();
    await flushPromises();

    expect(assignSpy).not.toHaveBeenCalled();
  });

  it('印があれば確かめさせずに移る', async () => {
    const { reauthQueries } = stubFetch({
      connections: [connection()],
      providers: [provider('google')],
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();

    expect(reauthQueries).toHaveLength(1);
    expect(assignSpy).toHaveBeenCalledWith(GOOGLE_LINK);
  });

  it('切れる間際の印は無いものとして確かめさせる', async () => {
    stubFetch({
      connections: [connection()],
      providers: [provider('google')],
      reauth: { ...FRESH, expires_at: new Date(new Date().getTime() + 5_000).toISOString() },
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();

    expect(assignSpy).not.toHaveBeenCalled();
    expect(dialog()?.textContent).toContain('本人確認');
  });

  it('GitHub App の連携は本人確認を問わずに移る', async () => {
    const { reauthQueries } = stubFetch({
      connections: [connection()],
      providers: [provider('github_app')],
      reauth: stale(),
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();

    expect(reauthQueries).toHaveLength(0);
    expect(assignSpy).toHaveBeenCalledTimes(1);
  });

  it('要否を問えなければ移らずに知らせる', async () => {
    stubFetch({
      connections: [connection()],
      providers: [provider('google')],
      reauth: { status: 500 },
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();

    expect(assignSpy).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('本人確認の状態を確かめられませんでした。');
  });

  it('連携で確かめに移る時は、続ける操作を印に置き、reauth を付けて移る', async () => {
    stubFetch({
      connections: [connection()],
      providers: [provider('google')],
      reauth: stale({
        password: false,
        oauth: [{ provider: 'github', connection_provider: 'github', instance_url: null }],
      }),
    });
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(false);
    await flushPromises();

    clickBodyButton('連携する');
    await flushPromises();
    testid('reauth-method-oauth').click();
    await flushPromises();
    expect(testid('reauth-oauth-note').textContent).toContain(
      'GitHub の画面へ移ります。確かめた後この画面へ戻り、Google の連携を追加する操作を続けます。',
    );
    testid('reauth-oauth-submit').click();
    await flushPromises();

    expect(assignSpy).toHaveBeenCalledWith(
      '/api/v1/auth/oauth/github?redirect_after=%2Fsettings%2Fsecurity&error_redirect_after=%2Fsettings%2Fsecurity&reauth=true',
    );
    expect(JSON.parse(window.sessionStorage.getItem(REAUTH_RESUME_NOTICE) ?? 'null')).toEqual({
      kind: 'link',
      provider: 'google',
      via: 'github',
    });
  });

  it('連携で確かめて戻ると、押し直さずに元の連携へ移る', async () => {
    stubFetch({ connections: [connection()], providers: [provider('google')] });
    window.sessionStorage.setItem(
      REAUTH_RESUME_NOTICE,
      JSON.stringify({ kind: 'link', provider: 'google', via: 'github' }),
    );
    window.history.replaceState({}, '', '/settings/security');
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const wrapper = mountSection(true);
    await flushPromises();

    expect(wrapper.text()).toContain('本人確認ができました。Google の連携へ進みます。');
    expect(assignSpy).toHaveBeenCalledWith(GOOGLE_LINK);
    // 印は一度きり
    expect(window.sessionStorage.getItem(REAUTH_RESUME_NOTICE)).toBeNull();
  });

  it('印が無ければ、戻ったように見える URL でも続けない', async () => {
    stubFetch({ connections: [connection()], providers: [provider('google')] });
    window.history.replaceState({}, '', '/settings/security');
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    mountSection(true);
    await flushPromises();

    expect(assignSpy).not.toHaveBeenCalled();
  });

  it('連携で確かめられずに戻ると、本人確認を出し直して知らせる', async () => {
    stubFetch({
      connections: [connection()],
      providers: [provider('google')],
      reauth: stale({
        oauth: [{ provider: 'github', connection_provider: 'github', instance_url: null }],
      }),
    });
    window.sessionStorage.setItem(
      REAUTH_RESUME_NOTICE,
      JSON.stringify({ kind: 'link', provider: 'google', via: 'github' }),
    );
    window.history.replaceState({}, '', '/settings/security?oauth_error=1');
    const assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
    const wrapper = mountSection(true);
    await flushPromises();

    expect(dialog()?.textContent).toContain(
      'GitHub で確かめられませんでした。連携しているアカウントで確かめるか、ほかの方法を選んでください。',
    );
    expect(wrapper.text()).not.toContain('外部プロバイダーでの連携に失敗しました。');
    expect(assignSpy).not.toHaveBeenCalled();
  });

  it('初回のパスワードは、欄を開く前に確かめさせ、確かめると欄が開く', async () => {
    stubFetch({
      connections: [connection()],
      providers: [],
      reauth: stale({ password: false, passkey: false, oauth: [] }),
    });
    mountSection(false);
    await flushPromises();

    clickBodyButton('パスワードを設定');
    await flushPromises();
    expect(document.body.querySelector('#password-next')).toBeNull();
    // 手段が無ければ、サインインし直す道だけを示す
    expect(dialog()?.textContent).toContain('使える方法がありません');
    expect(testid('reauth-sign-in-again')).toBeTruthy();
  });

  it('初回のパスワードを確かめた後は、押し直さずに欄が開く', async () => {
    stubFetch({
      connections: [connection()],
      providers: [],
      reauth: stale(),
      reauthPassword: 'Correct123!',
    });
    mountSection(false);
    await flushPromises();

    clickBodyButton('パスワードを設定');
    await flushPromises();
    await typeReauthPassword('Correct123!');
    testid('reauth-password-submit').click();
    await flushPromises();

    expect(document.body.querySelector('#password-next')).not.toBeNull();
  });

  it('連携で確かめて戻ると、初回のパスワードの欄を開く', async () => {
    stubFetch({ connections: [connection()], providers: [] });
    window.sessionStorage.setItem(
      REAUTH_RESUME_NOTICE,
      JSON.stringify({ kind: 'set-password', via: 'github' }),
    );
    window.history.replaceState({}, '', '/settings/security');
    const wrapper = mountSection(false);
    await flushPromises();

    expect(wrapper.text()).toContain('本人確認ができました。パスワードを設定してください。');
    expect(document.body.querySelector('#password-next')).not.toBeNull();
  });

  it('保存の前に期間が切れたら、確かめ直して打った字のまま撃ち直す', async () => {
    const { passwordBodies } = stubFetch({
      connections: [connection()],
      providers: [],
      passwordReauthRequired: 1,
      reauthPassword: 'Correct123!',
    });
    const wrapper = mountSection(false);
    await flushPromises();

    clickBodyButton('パスワードを設定');
    await flushPromises();
    await wrapper.find('#password-next').setValue('NewPassword123');
    await wrapper.find('#password-confirm').setValue('NewPassword123');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    // 撃った後に 403 を受け、本人確認を開いた
    expect(dialog()?.textContent).toContain('パスワードを設定する前に');

    await typeReauthPassword('Correct123!');
    testid('reauth-password-submit').click();
    await flushPromises();

    expect(passwordBodies).toEqual([
      { password: 'NewPassword123' },
      { password: 'NewPassword123' },
    ]);
    expect(document.body.textContent).toContain('パスワードを設定しました。');
  });
});

// テストケースの一覧
// - 要求の保存と復帰: 生の query で開くと保存して URL を `?resume=` に替える。サインインをまたいでも、
//   再読み込みしても、承認に失敗しても同じ中身を読める。承認か拒否が成功したときだけ消す
// - 未ログイン: `/me` が 401 の間は同意の API を呼ばず、戻り先付きのサインインへ送る
// - 表示: client_id の host と戻り先の host、枠の中・検査の失敗・候補 0 件では承認ボタンを出さない
// - loopback のクライアント: 注意書き、ポートまでの表示、テナント未選択、write:task を外す
// - テナントの候補: 所属するテナントだけ（Member・Viewer を含み、Guest を含まない）

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { Picker } from '@koyori-app/ui-vue';
import type { components } from '@/generated/api';
import Page from '../+Page.vue';

type Preview = components['schemas']['AuthorizationPreviewResponse'];
type Tenant = components['schemas']['TenantListItemResponse'];

const TENANT_ID = '11111111-1111-1111-1111-111111111111';
const SECOND_TENANT_ID = '44444444-4444-4444-4444-444444444444';
const CLIENT_ID = 'https://claude.ai/oauth/mcp-oauth-client-metadata';
const REDIRECT_URI = 'https://claude.ai/api/mcp/auth_callback';
const CHALLENGE = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
const ALL_SCOPES = [
  'read:tenant',
  'read:project',
  'read:task',
  'write:task',
  'read:milestone',
  'read:sprint',
] as const;

/** クライアントが付けてくる生の query。`:` を含むので、そのままではサインインの戻り先にできない */
const RAW_QUERY =
  `?response_type=code&client_id=${encodeURIComponent(CLIENT_ID)}` +
  `&redirect_uri=${encodeURIComponent(REDIRECT_URI)}` +
  `&code_challenge=${CHALLENGE}&code_challenge_method=S256&state=st-1&scope=read:task%20write:task`;

const EXPECTED_PARAMS = {
  client_id: CLIENT_ID,
  redirect_uri: REDIRECT_URI,
  response_type: 'code',
  code_challenge: CHALLENGE,
  code_challenge_method: 'S256',
  state: 'st-1',
  scope: 'read:task write:task',
  resource: null,
};

function tenant(overrides: Partial<Tenant> = {}): Tenant {
  return {
    id: TENANT_ID,
    display_id: 'acme',
    name: 'Acme Inc',
    description: '',
    icon_url: '',
    owner_id: '00000000-0000-0000-0000-000000000001',
    drive_quota_bytes: null,
    require_2fa: false,
    membership: 'Owner',
    member_role: null,
    ...overrides,
  };
}

const httpsPreview: Preview = {
  client_id: CLIENT_ID,
  client_name: 'Claude',
  client_host: 'claude.ai',
  redirect_host: 'claude.ai',
  redirect_is_loopback: false,
  scopes: [...ALL_SCOPES],
};

const loopbackPreview: Preview = {
  client_id: 'https://claude.ai/oauth/claude-code-client-metadata',
  client_name: 'Claude Code',
  client_host: 'claude.ai',
  redirect_host: '127.0.0.1:53124',
  redirect_is_loopback: true,
  scopes: [...ALL_SCOPES],
};

type MockState = {
  meStatus: number;
  tenants: Tenant[];
  preview: Preview;
  /** preview の失敗（状態と message） */
  previewError?: { status: number; message: string };
  approveError?: { status: number; message: string };
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

function stubFetch(overrides: Partial<MockState> = {}) {
  const state: MockState = {
    meStatus: 200,
    tenants: [tenant()],
    preview: httpsPreview,
    ...overrides,
  };
  const calls = {
    preview: [] as unknown[],
    approve: [] as unknown[],
    deny: [] as unknown[],
    tenants: 0,
  };
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => {
      const pathname = new URL(req.url, 'http://localhost').pathname;
      if (pathname.endsWith('/v1/auth/me')) {
        return state.meStatus === 200
          ? jsonResponse({ id: '00000000-0000-0000-0000-000000000001', username: 'tester' })
          : jsonResponse({ message: 'unauthorized' }, state.meStatus);
      }
      if (pathname.endsWith('/v1/tenants')) {
        calls.tenants += 1;
        return jsonResponse(state.tenants);
      }
      if (pathname.endsWith('/v1/oauth_grants/preview')) {
        calls.preview.push(await req.clone().json());
        return state.previewError
          ? jsonResponse({ message: state.previewError.message }, state.previewError.status)
          : jsonResponse(state.preview);
      }
      if (pathname.endsWith('/v1/oauth_grants/authorize')) {
        calls.approve.push(await req.clone().json());
        return state.approveError
          ? jsonResponse({ message: state.approveError.message }, state.approveError.status)
          : jsonResponse({ redirect_to: `${REDIRECT_URI}?code=the-code&state=st-1` });
      }
      if (pathname.endsWith('/v1/oauth_grants/deny')) {
        calls.deny.push(await req.clone().json());
        return jsonResponse({ redirect_to: `${REDIRECT_URI}?error=access_denied&state=st-1` });
      }
      return jsonResponse({ message: 'not-found' }, 404);
    }),
  );
  return { state, calls };
}

let replaceState: ReturnType<typeof vi.spyOn>;
let assign: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  sessionStorage.clear();
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  replaceState = vi.spyOn(window.history, 'replaceState').mockImplementation(() => {});
  assign = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
});

enableAutoUnmount(afterEach);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

async function settle() {
  for (let i = 0; i < 5; i += 1) {
    await vi.advanceTimersByTimeAsync(0);
    await flushPromises();
  }
}

/** `search` の query でページを開く。 */
async function openPage(search: string) {
  const wrapper = mount(Page, {
    global: {
      plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]],
      provide: { 'vike-vue:usePageContext': { urlParsed: { searchOriginal: search } } },
    },
    attachTo: document.body,
  });
  await settle();
  return wrapper;
}

/** 開いたときに替えた URL（`/oauth/authorize?resume=…`）。 */
function replacedUrl(): string {
  expect(replaceState).toHaveBeenCalledTimes(1);
  return String(replaceState.mock.calls[0]![2]);
}

function resumeSearchOf(url: string): string {
  return url.slice(url.indexOf('?'));
}

function button(wrapper: VueWrapper, label: string) {
  return wrapper.findAll('button').find((b) => b.text() === label);
}

function scopeCheckbox(wrapper: VueWrapper, scope: string): HTMLInputElement {
  const label = wrapper.findAll('label').find((l) => l.text() === scope);
  const input = label?.element.querySelector<HTMLInputElement>('input[type="checkbox"]');
  if (!input) throw new Error(`scope checkbox "${scope}" not found`);
  return input;
}

function selectTenants(wrapper: VueWrapper, ids: string[]) {
  (wrapper.findComponent(Picker).props('onSelectionChange') as (values: string[]) => void)(ids);
}

describe('/oauth/authorize: 要求の保存と復帰', () => {
  it('生の query で開くと保存して URL を替え、未ログインなら戻り先付きのサインインへ送る', async () => {
    const { calls } = stubFetch({ meStatus: 401 });

    await openPage(RAW_QUERY);

    const url = replacedUrl();
    expect(url).toMatch(/^\/oauth\/authorize\?resume=[A-Za-z0-9_-]+$/);
    // 戻り先は `/` に潰れていない
    expect(assign).toHaveBeenCalledWith(`/signin?redirect_after=${encodeURIComponent(url)}`);
    // `/me` が 401 の間は、ほかの API を呼ばない（共通の処理が戻り先なしのサインインへ送るため）
    expect(calls.preview).toHaveLength(0);
    expect(calls.tenants).toBe(0);
  });

  it('サインインから戻ると、保存した要求で preview を呼ぶ', async () => {
    stubFetch({ meStatus: 401 });
    const first = await openPage(RAW_QUERY);
    const url = replacedUrl();
    first.unmount();
    replaceState.mockClear();

    const { calls } = stubFetch();
    const wrapper = await openPage(resumeSearchOf(url));

    expect(calls.preview).toEqual([EXPECTED_PARAMS]);
    // 保存済みの要求で開いたときは、URL を替え直さない
    expect(replaceState).not.toHaveBeenCalled();
    expect(button(wrapper, '許可する')).toBeDefined();
  });

  it('同意画面を再読み込みしても、同じ中身が出る', async () => {
    const { calls } = stubFetch();
    const first = await openPage(RAW_QUERY);
    const url = replacedUrl();
    first.unmount();

    const wrapper = await openPage(resumeSearchOf(url));

    expect(calls.preview).toEqual([EXPECTED_PARAMS, EXPECTED_PARAMS]);
    expect(wrapper.get('[data-testid="client-host"]').text()).toBe('claude.ai');
  });

  it('承認に失敗したら保存を残し、もう一度送れる', async () => {
    const { state, calls } = stubFetch({
      approveError: { status: 403, message: '2fa-setup-required' },
    });
    const wrapper = await openPage(RAW_QUERY);
    const url = replacedUrl();

    await button(wrapper, '許可する')!.trigger('click');
    await settle();

    expect(wrapper.get('[role="alert"]').text()).toContain('2 段階認証を必須にしています');
    expect(assign).not.toHaveBeenCalled();
    wrapper.unmount();

    // 保存が残っているので、開き直しても同じ要求を読める
    state.approveError = undefined;
    const again = await openPage(resumeSearchOf(url));
    await button(again, '許可する')!.trigger('click');
    await settle();

    expect(calls.approve).toHaveLength(2);
    expect(assign).toHaveBeenCalledWith(`${REDIRECT_URI}?code=the-code&state=st-1`);
  });

  it('承認が成功したら保存を消し、同じ URL で開き直すとやり直しを案内する', async () => {
    const { calls } = stubFetch();
    const wrapper = await openPage(RAW_QUERY);
    const url = replacedUrl();

    await button(wrapper, '許可する')!.trigger('click');
    await settle();

    expect(calls.approve).toEqual([
      { ...EXPECTED_PARAMS, tenant_ids: [TENANT_ID], scopes: [...ALL_SCOPES] },
    ]);
    expect(assign).toHaveBeenCalledWith(`${REDIRECT_URI}?code=the-code&state=st-1`);
    expect(wrapper.text()).toContain('クライアントに戻っています');
    wrapper.unmount();

    const again = await openPage(resumeSearchOf(url));
    expect(again.text()).toContain('リンクが正しくありません');
    expect(again.text()).toContain('クライアントからやり直してください');
    expect(calls.preview).toHaveLength(1);
  });

  it('拒否すると deny の遷移先へ送り、保存を消す', async () => {
    const { calls } = stubFetch();
    const wrapper = await openPage(RAW_QUERY);
    const url = replacedUrl();

    await button(wrapper, '拒否する')!.trigger('click');
    await settle();

    expect(calls.deny).toEqual([EXPECTED_PARAMS]);
    expect(calls.approve).toHaveLength(0);
    expect(assign).toHaveBeenCalledWith(`${REDIRECT_URI}?error=access_denied&state=st-1`);
    wrapper.unmount();

    const again = await openPage(resumeSearchOf(url));
    expect(again.text()).toContain('リンクが正しくありません');
  });

  it.each(['?resume=a:b', '?resume=a..b', '?resume=a@b', '?resume=never-stashed', '', '?x=1'])(
    '読めない要求（%j）は、API を呼ばずにやり直しを案内する',
    async (search) => {
      const { calls } = stubFetch();
      // 文字種の合わないキーに中身が在っても読まない
      sessionStorage.setItem('oauth-authorize:a:b', RAW_QUERY.slice(1));

      const wrapper = await openPage(search);

      expect(wrapper.text()).toContain('リンクが正しくありません');
      expect(button(wrapper, '許可する')).toBeUndefined();
      expect(calls.preview).toHaveLength(0);
      expect(assign).not.toHaveBeenCalled();
    },
  );
});

describe('/oauth/authorize: 表示', () => {
  it('client_id の host、名乗る名前、戻り先の host を出す', async () => {
    stubFetch();
    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.get('h1').text()).toContain('claude.ai');
    expect(wrapper.get('[data-testid="client-name"]').text()).toBe('Claude');
    expect(wrapper.get('[data-testid="redirect-host"]').text()).toBe('claude.ai');
    expect(wrapper.text()).toContain('tester');
  });

  it('枠の中では承認ボタンを出さず、API も呼ばない', async () => {
    const { calls } = stubFetch();
    vi.stubGlobal('top', {});

    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.text()).toContain('このページは埋め込みでは開けません');
    expect(button(wrapper, '許可する')).toBeUndefined();
    expect(button(wrapper, '拒否する')).toBeUndefined();
    expect(calls.preview).toHaveLength(0);
  });

  it('枠の中では、未ログインでもサインインへ送らない', async () => {
    stubFetch({ meStatus: 401 });
    vi.stubGlobal('top', {});

    await openPage(RAW_QUERY);

    expect(assign).not.toHaveBeenCalled();
  });

  it.each([
    ['oauth-client-not-allowed', 'このサーバーでは許可されていないクライアントです。'],
    ['oauth-invalid-redirect-uri', '戻り先の URL が、クライアントの登録と一致しません。'],
  ])('preview の 400（%s）でエラーを出し、承認ボタンを出さない', async (message, text) => {
    stubFetch({ previewError: { status: 400, message } });

    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.get('[role="alert"]').text()).toBe(text);
    // 許可リストに足すときに要る client_id を出す
    expect(wrapper.get('[data-testid="rejected-client-id"]').text()).toBe(CLIENT_ID);
    expect(button(wrapper, '許可する')).toBeUndefined();
    expect(button(wrapper, '拒否する')).toBeUndefined();
    expect(assign).not.toHaveBeenCalled();
  });

  it('preview のサーバーエラーでも承認ボタンを出さない', async () => {
    stubFetch({ previewError: { status: 500, message: 'internal-error' } });

    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.get('[role="alert"]').text()).toContain('接続の要求を確かめられませんでした');
    expect(button(wrapper, '許可する')).toBeUndefined();
  });

  it('`/me` のサーバーエラーではサインインへ送らず、エラーを出す', async () => {
    stubFetch({ meStatus: 502 });

    const wrapper = await openPage(RAW_QUERY);
    await vi.advanceTimersByTimeAsync(10_000);
    await flushPromises();

    expect(assign).not.toHaveBeenCalled();
    expect(wrapper.get('[role="alert"]').text()).toContain('サーバーに接続できませんでした');
  });

  it('承認の 403（所属なし）と 400 を、文にして出す', async () => {
    const { state } = stubFetch({
      approveError: { status: 403, message: 'tenant-membership-missing' },
    });
    const wrapper = await openPage(RAW_QUERY);

    await button(wrapper, '許可する')!.trigger('click');
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe(
      '選択したテナントに所属していないため、接続できません。',
    );

    state.approveError = { status: 400, message: 'oauth-client-not-allowed' };
    await button(wrapper, '許可する')!.trigger('click');
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe(
      'このサーバーでは許可されていないクライアントです。',
    );
  });
});

describe('/oauth/authorize: テナントの候補', () => {
  it('所属するテナント（Member・Viewer を含む）を候補に出し、Guest は出さない', async () => {
    stubFetch({
      tenants: [
        tenant({ membership: 'Member', member_role: 'Member', name: 'As Member' }),
        tenant({
          id: SECOND_TENANT_ID,
          membership: 'Member',
          member_role: 'Viewer',
          name: 'As Viewer',
        }),
        tenant({
          id: '55555555-5555-5555-5555-555555555555',
          membership: 'Guest',
          require_2fa: null,
          name: 'As Guest',
        }),
      ],
    });

    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.findComponent(Picker).props('items')).toEqual([
      { value: TENANT_ID, label: 'As Member' },
      { value: SECOND_TENANT_ID, label: 'As Viewer' },
    ]);
    // 候補が 2 つ以上なら未選択から始め、選ぶまで承認できない
    expect(wrapper.findComponent(Picker).props('selectedValues')).toEqual([]);
    expect(button(wrapper, '許可する')!.attributes('disabled')).toBeDefined();
  });

  it('選んだテナントを承認に載せる', async () => {
    const { calls } = stubFetch({
      tenants: [tenant(), tenant({ id: SECOND_TENANT_ID, name: 'Beta LLC' })],
    });
    const wrapper = await openPage(RAW_QUERY);

    selectTenants(wrapper, [TENANT_ID, SECOND_TENANT_ID]);
    await flushPromises();
    await button(wrapper, '許可する')!.trigger('click');
    await settle();

    expect((calls.approve[0] as { tenant_ids: string[] }).tenant_ids).toEqual([
      TENANT_ID,
      SECOND_TENANT_ID,
    ]);
  });

  it('候補が Guest だけなら、承認ボタンを出さずに理由を出す', async () => {
    stubFetch({ tenants: [tenant({ membership: 'Guest', require_2fa: null })] });

    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.get('[data-testid="no-tenants"]').text()).toBe(
      '接続できるテナントがありません。テナントに所属すると接続できます。',
    );
    expect(button(wrapper, '許可する')).toBeUndefined();
    // 拒否はできる
    expect(button(wrapper, '拒否する')).toBeDefined();
  });
});

describe('/oauth/authorize: scope', () => {
  it('チェックを外した scope は送らず、全部外すと承認できない', async () => {
    const { calls } = stubFetch({
      preview: { ...httpsPreview, scopes: ['read:task', 'write:task'] },
    });
    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.findAll('input[type="checkbox"]')).toHaveLength(2);
    scopeCheckbox(wrapper, 'write:task').click();
    scopeCheckbox(wrapper, 'read:task').click();
    await flushPromises();
    expect(button(wrapper, '許可する')!.attributes('disabled')).toBeDefined();
    await button(wrapper, '許可する')!.trigger('click');
    await settle();
    expect(calls.approve).toHaveLength(0);

    scopeCheckbox(wrapper, 'read:task').click();
    await flushPromises();
    await button(wrapper, '許可する')!.trigger('click');
    await settle();
    expect((calls.approve[0] as { scopes: string[] }).scopes).toEqual(['read:task']);
  });
});

describe('/oauth/authorize: loopback のクライアント', () => {
  it('注意書きとポートまでの戻り先を出し、テナント未選択・write:task を外した状態で始める', async () => {
    const { calls } = stubFetch({ preview: loopbackPreview });

    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.get('[data-testid="loopback-warning"]').text()).toContain(
      'この端末で動くアプリが接続を求めています。',
    );
    expect(wrapper.get('[data-testid="loopback-warning"]').text()).toContain('拒否してください。');
    expect(wrapper.get('[data-testid="redirect-host"]').text()).toBe('127.0.0.1:53124');
    // 候補が 1 件でも未選択で、承認ボタンは押せない
    expect(wrapper.findComponent(Picker).props('selectedValues')).toEqual([]);
    expect(button(wrapper, '許可する')!.attributes('disabled')).toBeDefined();
    expect(scopeCheckbox(wrapper, 'write:task').checked).toBe(false);
    for (const scope of ALL_SCOPES.filter((s) => s !== 'write:task')) {
      expect(scopeCheckbox(wrapper, scope).checked, scope).toBe(true);
    }

    await button(wrapper, '許可する')!.trigger('click');
    await settle();
    expect(calls.approve).toHaveLength(0);

    // テナントを選ぶと承認でき、送る scopes に write:task は無い
    selectTenants(wrapper, [TENANT_ID]);
    await flushPromises();
    expect(button(wrapper, '許可する')!.attributes('disabled')).toBeUndefined();
    await button(wrapper, '許可する')!.trigger('click');
    await settle();

    expect(calls.approve).toHaveLength(1);
    const body = calls.approve[0] as { tenant_ids: string[]; scopes: string[] };
    expect(body.tenant_ids).toEqual([TENANT_ID]);
    expect(body.scopes).toEqual(ALL_SCOPES.filter((s) => s !== 'write:task'));
  });

  it('write:task は、自分でチェックを入れれば送る', async () => {
    const { calls } = stubFetch({ preview: loopbackPreview });
    const wrapper = await openPage(RAW_QUERY);

    scopeCheckbox(wrapper, 'write:task').click();
    selectTenants(wrapper, [TENANT_ID]);
    await flushPromises();
    await button(wrapper, '許可する')!.trigger('click');
    await settle();

    expect((calls.approve[0] as { scopes: string[] }).scopes).toEqual([...ALL_SCOPES]);
  });

  it('write:task を求めていない要求では、何も外さない', async () => {
    stubFetch({ preview: { ...loopbackPreview, scopes: ['read:task', 'read:project'] } });
    const wrapper = await openPage(RAW_QUERY);

    expect(scopeCheckbox(wrapper, 'read:task').checked).toBe(true);
    expect(scopeCheckbox(wrapper, 'read:project').checked).toBe(true);
  });

  it('対照: https のクライアントは注意書きを出さず、候補 1 件は選択済み、write:task はチェック済み', async () => {
    stubFetch();

    const wrapper = await openPage(RAW_QUERY);

    expect(wrapper.find('[data-testid="loopback-warning"]').exists()).toBe(false);
    expect(wrapper.findComponent(Picker).props('selectedValues')).toEqual([TENANT_ID]);
    expect(scopeCheckbox(wrapper, 'write:task').checked).toBe(true);
    expect(button(wrapper, '許可する')!.attributes('disabled')).toBeUndefined();
  });
});

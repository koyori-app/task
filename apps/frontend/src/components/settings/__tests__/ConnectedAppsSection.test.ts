// テストケースの一覧
// - 一覧（名前・client_id の host・テナント名・スコープ数・期限・最終使用）、0 件の文、取得の失敗
// - 解除: 確認のダイアログを経て DELETE を送り、一覧を取り直す。キャンセルと失敗では一覧が残る

import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import type { components } from '@/generated/api';
import ConnectedAppsSection from '../ConnectedAppsSection.vue';

type Grant = components['schemas']['OAuthGrantResponse'];

const TENANT_ID = '11111111-1111-1111-1111-111111111111';
const SECOND_TENANT_ID = '44444444-4444-4444-4444-444444444444';
const CLAUDE_GRANT_ID = '22222222-2222-2222-2222-222222222222';
const CODEX_GRANT_ID = '33333333-3333-3333-3333-333333333333';

function grant(overrides: Partial<Grant> = {}): Grant {
  return {
    id: CLAUDE_GRANT_ID,
    client_id: 'https://claude.ai/oauth/mcp-oauth-client-metadata',
    client_name: 'Claude',
    tenant_ids: [TENANT_ID],
    scopes: ['read:tenant', 'read:project', 'read:task'],
    expires_at: '2099-11-14T00:00:00Z',
    last_used_at: null,
    ...overrides,
  };
}

const codexGrant = grant({
  id: CODEX_GRANT_ID,
  client_id: 'https://chatgpt.com/oauth/codex/client.json',
  client_name: 'Codex',
  tenant_ids: [TENANT_ID, SECOND_TENANT_ID],
  scopes: ['read:task'],
  last_used_at: '2026-01-01T00:00:00Z',
});

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

function stubFetch(state: { grants: Grant[]; listStatus?: number; deleteStatus?: number }) {
  const deletedPaths: string[] = [];
  let listCalls = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (req: Request) => {
      const pathname = new URL(req.url, 'http://localhost').pathname;
      if (req.method === 'GET' && pathname.endsWith('/v1/tenants')) {
        return jsonResponse([
          { id: TENANT_ID, name: 'Acme Inc', membership: 'Owner' },
          { id: SECOND_TENANT_ID, name: 'Beta LLC', membership: 'Member' },
        ]);
      }
      if (req.method === 'GET' && pathname.endsWith('/v1/oauth_grants')) {
        listCalls += 1;
        return state.listStatus
          ? jsonResponse({ message: 'internal-error' }, state.listStatus)
          : jsonResponse(state.grants);
      }
      if (req.method === 'DELETE' && pathname.includes('/v1/oauth_grants/')) {
        deletedPaths.push(pathname);
        if (state.deleteStatus) return jsonResponse({ message: 'error' }, state.deleteStatus);
        const id = pathname.split('/').pop();
        state.grants = state.grants.filter((g) => g.id !== id);
        return new Response(null, { status: 204 });
      }
      return jsonResponse({ message: 'not-found' }, 404);
    }),
  );
  return { deletedPaths, listCalls: () => listCalls };
}

function mountSection() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(ConnectedAppsSection, {
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function bodyButtons(label: string) {
  return [...document.body.querySelectorAll('button')].filter(
    (b) => b.textContent?.trim() === label,
  );
}

function clickBodyButton(label: string, index = 0) {
  const button = bodyButtons(label)[index];
  if (!button) throw new Error(`button "${label}" not found`);
  button.click();
}

enableAutoUnmount(afterEach);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('ConnectedAppsSection', () => {
  it('名前・client_id の host・テナント名・スコープ数・期限・最終使用を一覧に出す', async () => {
    stubFetch({ grants: [grant(), codexGrant] });
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.get('h2').text()).toBe('連携中のアプリ');
    const rows = wrapper.findAll('[data-testid="connected-app-list"] > li');
    expect(rows).toHaveLength(2);
    expect(rows[0]!.text()).toContain('Claude');
    expect(rows[0]!.text()).toContain('claude.ai');
    expect(rows[0]!.text()).toContain('Acme Inc');
    expect(rows[0]!.text()).toContain('3 スコープ');
    expect(rows[0]!.text()).toContain('まで有効');
    expect(rows[0]!.text()).toContain('未使用');
    // 名前は自己申告なので、どこの client_id かを host で見分けられる
    expect(rows[1]!.text()).toContain('Codex');
    expect(rows[1]!.text()).toContain('chatgpt.com');
    expect(rows[1]!.text()).toContain('Acme Inc、Beta LLC');
    expect(rows[1]!.text()).toContain('1 スコープ');
    expect(rows[1]!.text()).not.toContain('未使用');
  });

  it('0 件なら「連携中のアプリはありません。」を出す', async () => {
    stubFetch({ grants: [] });
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).toContain('連携中のアプリはありません。');
    expect(wrapper.find('[data-testid="connected-app-list"]').exists()).toBe(false);
    // ページなので、節そのものは消さない
    expect(wrapper.get('h2').text()).toBe('連携中のアプリ');
  });

  it('一覧を取得できなければ、エラーを出す', async () => {
    stubFetch({ grants: [], listStatus: 500 });
    const wrapper = mountSection();
    await flushPromises();

    expect(wrapper.text()).toContain('連携中のアプリの一覧を取得できませんでした。');
    expect(wrapper.text()).not.toContain('連携中のアプリはありません。');
  });

  it('解除は確認のダイアログを経て DELETE を送り、一覧を取り直す', async () => {
    const { deletedPaths, listCalls } = stubFetch({ grants: [grant(), codexGrant] });
    const wrapper = mountSection();
    await flushPromises();
    expect(listCalls()).toBe(1);

    clickBodyButton('接続を解除', 1);
    await flushPromises();

    expect(document.body.textContent).toContain('接続を解除しますか？');
    expect(document.body.textContent).toContain('「Codex」（chatgpt.com）');
    expect(document.body.textContent).toContain('「Acme Inc、Beta LLC」');
    // 確かめる前には送らない
    expect(deletedPaths).toHaveLength(0);

    clickBodyButton('解除する');
    await flushPromises();

    expect(deletedPaths).toEqual([`/api/v1/oauth_grants/${CODEX_GRANT_ID}`]);
    expect(listCalls()).toBe(2);
    const rows = wrapper.findAll('[data-testid="connected-app-list"] > li');
    expect(rows).toHaveLength(1);
    expect(rows[0]!.text()).toContain('Claude');
  });

  it('キャンセルすると何も送らず、一覧は残る', async () => {
    const { deletedPaths } = stubFetch({ grants: [grant()] });
    const wrapper = mountSection();
    await flushPromises();

    clickBodyButton('接続を解除');
    await flushPromises();
    clickBodyButton('キャンセル');
    await flushPromises();

    expect(deletedPaths).toHaveLength(0);
    expect(wrapper.findAll('[data-testid="connected-app-list"] > li')).toHaveLength(1);
  });

  it('解除に失敗したらエラーを出し、一覧は残る', async () => {
    stubFetch({ grants: [grant()], deleteStatus: 500 });
    const wrapper = mountSection();
    await flushPromises();

    clickBodyButton('接続を解除');
    await flushPromises();
    clickBodyButton('解除する');
    await flushPromises();

    expect(document.body.textContent).toContain('接続を解除できませんでした。');
    expect(wrapper.findAll('[data-testid="connected-app-list"] > li')).toHaveLength(1);
  });
});

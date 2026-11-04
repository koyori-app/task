import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { flushPromises, mount, enableAutoUnmount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  post: vi.fn(),
  patch: vi.fn(),
  remove: vi.fn(),
  bots: {
    value: [
      {
        id: 'bot-1',
        username: 'pipeline',
        role: 'Member',
        account_kind: 'bot',
        is_suspended: false,
      },
    ],
  },
  tokens: {
    value: [
      {
        id: 'token-1',
        name: 'deploy',
        token_last_four: 'abcd',
        scopes: ['read:task'],
        expires_at: '2027-01-01T00:00:00Z',
        last_used_at: null,
      },
    ],
  },
}));
vi.mock('@/lib/api-vue-query', () => ({
  apiClient: {
    queryOptions: (_method: string, path: string, init: unknown) => ({
      queryKey: ['get', path, init],
      queryFn: async () => mocks.tokens.value,
      initialData: mocks.tokens.value,
    }),
    useQuery: (_method: string, path: string) => ({
      data: path.endsWith('/bots')
        ? mocks.bots
        : path.endsWith('/tokens')
          ? mocks.tokens
          : { value: [{ id: 'project-1', name: 'release' }] },
      isPending: { value: false },
      isError: { value: false },
      refetch: vi.fn(),
    }),
  },
  fetchClient: { POST: mocks.post, PATCH: mocks.patch, DELETE: mocks.remove },
}));
import TenantBotsSection from '../TenantBotsSection.vue';

enableAutoUnmount(afterEach);

function render() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = mount(TenantBotsSection, {
    props: { tenantId: 'tenant-1' },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
  return { wrapper, queryClient };
}

function bodyButton(label: string) {
  const button = [...document.body.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!button) throw new Error(`button "${label}" not found`);
  return button;
}

async function openTokens() {
  bodyButton('PAT を管理').click();
  await flushPromises();
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.bots.value[0]!.is_suspended = false;
  mocks.post.mockResolvedValue({ data: { id: 'bot-2' }, response: { ok: true } });
  mocks.patch.mockResolvedValue({ response: { ok: true } });
  mocks.remove.mockResolvedValue({ response: { ok: true } });
});

describe('テナントの BOT 管理', () => {
  it('BOT の作成で名前と既存ロールだけを渡す', async () => {
    const { wrapper } = render();
    await wrapper.get('#bot-name').setValue(' nightly ');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(mocks.post).toHaveBeenCalledWith(
      '/v1/tenants/{tenant_id}/bots',
      expect.objectContaining({
        params: { path: { tenant_id: 'tenant-1' } },
        body: { username: 'nightly', role: 'Member' },
      }),
    );
    expect((wrapper.get('#bot-name').element as HTMLInputElement).value).toBe('');
  });

  it('PAT は期限とスコープを持ち、発行値は閉じると消え、query の cache にも残らない', async () => {
    mocks.post.mockResolvedValue({ data: { token: 'one-time-secret' }, response: { ok: true } });
    const { wrapper, queryClient } = render();
    await openTokens();
    await wrapper.get('#bot-token-name').setValue('build');
    await wrapper.findAll('form')[1]!.trigger('submit');
    await flushPromises();
    expect(mocks.post).toHaveBeenCalledWith(
      '/v1/tenants/{tenant_id}/bots/{bot_id}/tokens',
      expect.objectContaining({
        body: {
          name: 'build',
          scopes: ['read:task'],
          project_ids: null,
          expires_at: expect.any(String),
        },
      }),
    );
    const expiry = new Date(mocks.post.mock.calls[0]![1].body.expires_at).getTime();
    expect(expiry).toBeGreaterThan(new Date().getTime());
    expect(expiry).toBeLessThanOrEqual(new Date().getTime() + 365 * 86400000);
    expect(wrapper.get('[data-testid="created-bot-token"]').text()).toBe('one-time-secret');
    expect(JSON.stringify(queryClient.getQueryCache().getAll())).not.toContain('one-time-secret');
    bodyButton('閉じる').click();
    await flushPromises();
    expect(wrapper.html()).not.toContain('one-time-secret');
  });

  it('停止は選んだ BOT の API を呼び、PAT の取り消しは確かめてから呼ぶ', async () => {
    render();
    bodyButton('停止').click();
    await flushPromises();
    expect(mocks.patch).toHaveBeenCalledWith(
      '/v1/tenants/{tenant_id}/bots/{bot_id}',
      expect.objectContaining({
        params: { path: { tenant_id: 'tenant-1', bot_id: 'bot-1' } },
        body: { is_suspended: true },
      }),
    );
    await openTokens();
    bodyButton('取り消し').click();
    await flushPromises();
    expect(mocks.remove).not.toHaveBeenCalled();
    bodyButton('取り消す').click();
    await flushPromises();
    expect(mocks.remove).toHaveBeenCalledWith(
      '/v1/tenants/{tenant_id}/bots/{bot_id}/tokens/{token_id}',
      { params: { path: { tenant_id: 'tenant-1', bot_id: 'bot-1', token_id: 'token-1' } } },
    );
  });

  it('停止中の BOT には PAT を発行できない', async () => {
    mocks.bots.value[0]!.is_suspended = true;
    const { wrapper } = render();
    await openTokens();
    await wrapper.get('#bot-token-name').setValue('build');
    expect(bodyButton('PAT を発行').disabled).toBe(true);
  });

  it('失敗時に成功した表示へ進まない', async () => {
    mocks.post.mockResolvedValue({ error: { message: 'denied' }, response: { ok: false } });
    const { wrapper } = render();
    await wrapper.get('#bot-name').setValue('retry-bot');
    await wrapper.get('form').trigger('submit');
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('操作に失敗');
    expect((wrapper.get('#bot-name').element as HTMLInputElement).value).toBe('retry-bot');
  });
});

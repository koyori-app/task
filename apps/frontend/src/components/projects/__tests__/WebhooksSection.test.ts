import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient, useQuery } from '@tanstack/vue-query';
import { unref, type Ref } from 'vue';

const { mutateAsync, queryState } = vi.hoisted(() => ({
  mutateAsync: {
    create: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
    redeliver: vi.fn(),
  },
  queryState: {
    webhooks: [] as unknown[],
    deliveries: [] as unknown[],
    isPending: false,
    isError: false,
    error: null as unknown,
  },
}));

// 一覧と配信履歴は useQuery に渡る queryKey のパスで出し分ける
vi.mock('@tanstack/vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tanstack/vue-query')>();
  return {
    ...actual,
    useQuery: vi.fn((options: Ref<{ queryKey: readonly unknown[] }>) => {
      const isDeliveries = String(unref(options).queryKey[1]).endsWith('/deliveries');
      return isDeliveries
        ? {
            data: { value: queryState.deliveries },
            isPending: { value: false },
            isError: { value: false },
          }
        : {
            data: { value: queryState.webhooks },
            isPending: { value: queryState.isPending },
            isError: { value: queryState.isError },
            error: { value: queryState.error },
          };
    }),
  };
});

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn((method: string, path: string) => ({
        mutateAsync:
          method === 'put'
            ? mutateAsync.update
            : method === 'delete'
              ? mutateAsync.delete
              : path.endsWith('/redeliver')
                ? mutateAsync.redeliver
                : mutateAsync.create,
        isPending: { value: false },
      })),
    },
  };
});

import WebhooksSection from '../WebhooksSection.vue';
import type { components } from '@/generated/api';

type WebhookResponse = components['schemas']['WebhookResponse'];
type WebhookDeliveryResponse = components['schemas']['WebhookDeliveryResponse'];

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const activeHook: WebhookResponse = {
  id: '00000000-0000-4000-8000-000000000031',
  project_id: PROJECT_UUID,
  url: 'https://example.com/hook',
  events: ['task.created', 'comment.created'],
  format: 'json',
  is_active: true,
  failure_streak: 2,
  created_by: '00000000-0000-4000-8000-000000000001',
  created_at: '2026-09-01T00:00:00Z',
};

const stoppedHook: WebhookResponse = {
  ...activeHook,
  id: '00000000-0000-4000-8000-000000000032',
  url: 'https://discord.com/api/webhooks/1/abc',
  events: ['review.round_created'],
  format: 'discord',
  is_active: false,
  failure_streak: 5,
};

const failedDelivery: WebhookDeliveryResponse = {
  id: '00000000-0000-4000-8000-000000000041',
  webhook_id: activeHook.id,
  event: 'task.created',
  payload: {},
  status_code: 500,
  attempt: 5,
  next_attempt_at: null,
  last_error: 'HTTP 500',
  delivered_at: null,
  created_at: '2026-09-02T00:00:00Z',
};

function mountView() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  mount(WebhooksSection, {
    props: { tenantId: TENANT_UUID, projectId: PROJECT_UUID },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
  return { invalidate };
}

function row(url: string) {
  const el = [...document.body.querySelectorAll('[data-testid="webhook-row"]')].find((r) =>
    r.textContent?.includes(url),
  );
  if (!el) throw new Error(`row "${url}" not found`);
  return el;
}

function clickButton(label: string, root: ParentNode = document.body) {
  const button = [...root.querySelectorAll('button')].find((b) => b.textContent?.trim() === label);
  if (!button) throw new Error(`button "${label}" not found`);
  button.click();
}

describe('WebhooksSection', () => {
  beforeEach(() => {
    Object.values(mutateAsync).forEach((fn) => fn.mockReset());
    vi.mocked(useQuery).mockClear();
    queryState.webhooks = [];
    queryState.deliveries = [];
    queryState.isPending = false;
    queryState.isError = false;
    queryState.error = null;
    document.body.innerHTML = '';
  });

  it('Webhook を一覧表示し、連続失敗で止まったものは停止と表示する', async () => {
    queryState.webhooks = [activeHook, stoppedHook];
    mountView();
    await flushPromises();

    const active = row(activeHook.url).textContent!;
    expect(active).toContain('JSON');
    expect(active).toContain('タスクの作成');
    expect(active).toContain('コメントの投稿');
    expect(active).toContain('有効');
    expect(active).toContain('連続失敗 2 回');
    expect(active).toContain('無効にする');

    const stopped = row(stoppedHook.url).textContent!;
    expect(stopped).toContain('Discord');
    expect(stopped).toContain('レビューラウンドの起票');
    expect(stopped).toContain('無効');
    expect(stopped).toContain('連続失敗で停止');
    expect(stopped).toContain('有効にする');
  });

  it('0 件なら空状態を表示する', async () => {
    mountView();
    await flushPromises();

    expect(document.body.textContent).toContain('Webhook はまだありません');
  });

  it('読み込みに失敗したらエラーを表示する', async () => {
    queryState.isError = true;
    queryState.error = { response: { status: 500 } };
    mountView();
    await flushPromises();

    expect(document.body.querySelector('[role="alert"]')?.textContent).toContain(
      'Webhook を読み込めませんでした',
    );
  });

  it('「無効にする」で is_active: false を PUT し、一覧を再取得する', async () => {
    mutateAsync.update.mockResolvedValue({ ...activeHook, is_active: false });
    queryState.webhooks = [activeHook];
    const { invalidate } = mountView();
    await flushPromises();

    clickButton('無効にする', row(activeHook.url));
    await flushPromises();

    expect(mutateAsync.update).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: activeHook.id } },
      body: { is_active: false },
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks'],
    });
  });

  it('403 はプロジェクト管理者権限が必要だと表示する', async () => {
    mutateAsync.update.mockRejectedValue({ response: { status: 403 } });
    queryState.webhooks = [activeHook];
    mountView();
    await flushPromises();

    clickButton('無効にする', row(activeHook.url));
    await flushPromises();

    expect(document.body.querySelector('[role="alert"]')?.textContent).toContain(
      'この操作にはプロジェクトの管理者権限が必要です',
    );
  });

  it('削除は確認ダイアログを経て DELETE を送る', async () => {
    mutateAsync.delete.mockResolvedValue(undefined);
    queryState.webhooks = [activeHook];
    mountView();
    await flushPromises();

    clickButton('削除', row(activeHook.url));
    await flushPromises();
    expect(document.body.textContent).toContain('Webhook を削除しますか？');
    expect(mutateAsync.delete).not.toHaveBeenCalled();

    clickButton('削除する');
    await flushPromises();

    expect(mutateAsync.delete).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: activeHook.id } },
    });
    expect(document.body.textContent).not.toContain('Webhook を削除しますか？');
  });

  it('配信履歴を展開すると取得し、「再送」で POST して履歴を再取得する', async () => {
    mutateAsync.redeliver.mockResolvedValue({ ...failedDelivery, id: 'new' });
    queryState.webhooks = [activeHook];
    queryState.deliveries = [failedDelivery];
    const { invalidate } = mountView();
    await flushPromises();

    const deliveriesOptions = vi
      .mocked(useQuery)
      .mock.calls.map((call) => call[0] as unknown as Ref<Record<string, unknown>>)
      .find((options) => String((options.value.queryKey as unknown[])[1]).endsWith('/deliveries'))!;
    expect(deliveriesOptions.value.enabled).toBe(false);

    clickButton('配信履歴', row(activeHook.url));
    await flushPromises();

    expect(deliveriesOptions.value.enabled).toBe(true);
    const key = JSON.stringify(deliveriesOptions.value.queryKey);
    expect(key).toContain(activeHook.id);
    expect(key).toContain('"limit":20');
    const list = document.body.querySelector('[data-testid="delivery-list"]')!;
    expect(list.textContent).toContain('失敗 (5 回)');
    expect(list.textContent).toContain('HTTP 500');

    clickButton('再送', list);
    await flushPromises();

    expect(mutateAsync.redeliver).toHaveBeenCalledWith({
      params: {
        path: {
          tenant_id: TENANT_UUID,
          project_id: PROJECT_UUID,
          id: activeHook.id,
          delivery_id: failedDelivery.id,
        },
      },
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: ['get', '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks/{id}/deliveries'],
    });
  });
});

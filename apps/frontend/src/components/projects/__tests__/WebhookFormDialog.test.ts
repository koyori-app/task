import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, DOMWrapper, enableAutoUnmount } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

const { createMutateAsync, updateMutateAsync } = vi.hoisted(() => ({
  createMutateAsync: vi.fn(),
  updateMutateAsync: vi.fn(),
}));

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  return {
    ...actual,
    apiClient: {
      ...actual.apiClient,
      useMutation: vi.fn((method: string) => ({
        mutateAsync: method === 'post' ? createMutateAsync : updateMutateAsync,
        isPending: { value: false },
      })),
    },
  };
});

import WebhookFormDialog from '../WebhookFormDialog.vue';
import type { components } from '@/generated/api';

type WebhookResponse = components['schemas']['WebhookResponse'];

enableAutoUnmount(afterEach);

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const existing: WebhookResponse = {
  id: '00000000-0000-4000-8000-000000000031',
  project_id: PROJECT_UUID,
  url: 'https://example.com/hook',
  events: ['task.created'],
  format: 'json',
  is_active: true,
  failure_streak: 0,
  created_by: '00000000-0000-4000-8000-000000000001',
  created_at: '2026-09-01T00:00:00Z',
};

function mountDialog(webhook: WebhookResponse | null = null) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return mount(WebhookFormDialog, {
    props: { tenantId: TENANT_UUID, projectId: PROJECT_UUID, webhook },
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  });
}

function input(id: string) {
  const el = document.body.querySelector<HTMLInputElement>(`#${id}`);
  if (!el) throw new Error(`input #${id} not found`);
  return new DOMWrapper(el);
}

function checkEvent(label: string) {
  const box = document.body.querySelector<HTMLButtonElement>(
    `button[role="checkbox"][aria-label="${label}"]`,
  );
  if (!box) throw new Error(`checkbox "${label}" not found`);
  box.click();
}

async function submit() {
  await new DOMWrapper(document.body.querySelector('form')!).trigger('submit');
  await flushPromises();
}

describe('WebhookFormDialog', () => {
  beforeEach(() => {
    createMutateAsync.mockReset();
    updateMutateAsync.mockReset();
    document.body.innerHTML = '';
  });

  it('secret が 15 文字なら送信せずエラーを表示し、16 文字なら送信する', async () => {
    createMutateAsync.mockResolvedValue({ ...existing, secret: 'a'.repeat(16) });
    mountDialog();
    await flushPromises();

    await input('webhook-url').setValue('https://example.com/hook');
    await input('webhook-secret').setValue('a'.repeat(15));
    checkEvent('タスクの作成');
    await flushPromises();
    await submit();

    expect(createMutateAsync).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('シークレットは 16 文字以上で入力してください');

    await input('webhook-secret').setValue('a'.repeat(16));
    await submit();

    expect(createMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID } },
      body: {
        url: 'https://example.com/hook',
        secret: 'a'.repeat(16),
        events: ['task.created'],
        format: 'json',
      },
    });
  });

  it('イベントが 0 個なら送信せずエラーを表示する', async () => {
    mountDialog();
    await flushPromises();

    await input('webhook-url').setValue('https://example.com/hook');
    await input('webhook-secret').setValue('a'.repeat(16));
    await submit();

    expect(createMutateAsync).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('イベントを 1 つ以上選んでください');
  });

  it('作成に成功したら secret を一度だけ表示する', async () => {
    createMutateAsync.mockResolvedValue({ ...existing, secret: 'returned-secret-0123456789' });
    mountDialog();
    await flushPromises();

    await input('webhook-url').setValue('https://example.com/hook');
    await input('webhook-secret').setValue('returned-secret-0123456789');
    checkEvent('コメントの投稿');
    await flushPromises();
    await submit();

    expect(document.body.textContent).toContain('Webhook を作成しました');
    expect(document.body.querySelector('[data-testid="created-secret"]')?.textContent).toBe(
      'returned-secret-0123456789',
    );
    expect(document.body.querySelector('form')).toBeNull();
  });

  it('API の 400 は message をそのまま表示する', async () => {
    createMutateAsync.mockRejectedValue({
      response: { status: 400 },
      error: { message: 'url を送信先に使えません: private address' },
    });
    mountDialog();
    await flushPromises();

    await input('webhook-url').setValue('https://10.0.0.1/hook');
    await input('webhook-secret').setValue('a'.repeat(16));
    checkEvent('タスクの作成');
    await flushPromises();
    await submit();

    expect(document.body.querySelector('[role="alert"]')?.textContent).toContain(
      'url を送信先に使えません: private address',
    );
  });

  it('編集で secret が空なら body に secret を含めず、変えたフィールドだけ送る', async () => {
    updateMutateAsync.mockResolvedValue(existing);
    const wrapper = mountDialog(existing);
    await flushPromises();

    expect((input('webhook-url').element as HTMLInputElement).value).toBe(existing.url);
    checkEvent('レビュー指摘の状態変更');
    await flushPromises();
    await submit();

    expect(updateMutateAsync).toHaveBeenCalledWith({
      params: { path: { tenant_id: TENANT_UUID, project_id: PROJECT_UUID, id: existing.id } },
      body: { events: ['task.created', 'review.finding_changed'] },
    });
    expect(wrapper.emitted('close')).toHaveLength(1);
  });
});

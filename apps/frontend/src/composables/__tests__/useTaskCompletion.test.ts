import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent } from 'vue';
import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import type { paths } from '@/generated/api';
import { useTaskCompletion } from '../useTaskCompletion';

const { control, requestLog, fetchMock } = vi.hoisted(() => {
  function jsonResponse(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  /** 次の書き込み（PUT / POST）の失敗。null なら成功させる */
  const control: { fail: { status: number; message: string } | null } = { fail: null };
  const requestLog: { method: string; url: string; body?: unknown }[] = [];

  const fetchMock = async (input: Request) => {
    const method = input.method.toUpperCase();
    const text = await input.clone().text();
    requestLog.push({ method, url: input.url, body: text ? JSON.parse(text) : undefined });
    if (method === 'GET') return jsonResponse({ tasks: [], total: 0 });
    if (control.fail) return jsonResponse({ message: control.fail.message }, control.fail.status);
    return jsonResponse({ id: 'task-1' });
  };

  return { control, requestLog, fetchMock };
});

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  const { default: createFetchClient } = await import('openapi-fetch');
  const { createClient } = await import('@koyori-app/openapi-vue-query');
  const testFetchClient = createFetchClient<paths>({
    baseUrl: 'http://test.local/api',
    fetch: (req: Request) => fetchMock(req),
  });
  return {
    ...actual,
    fetchClient: testFetchClient,
    apiClient: createClient<paths>(testFetchClient),
  };
});

describe('useTaskCompletion', () => {
  let completion: ReturnType<typeof useTaskCompletion>;
  const task = { id: 'task-1', require_all_assignees_done: false } as never;
  const writes = () => requestLog.filter((entry) => entry.method !== 'GET');

  beforeEach(() => {
    control.fail = null;
    requestLog.length = 0;
    const Host = defineComponent({
      setup() {
        completion = useTaskCompletion({ tenantId: 'tenant-1', projectId: 'project-1' });
        return () => null;
      },
    });
    mount(Host, {
      global: {
        plugins: [
          [
            VueQueryPlugin,
            {
              queryClient: new QueryClient({
                defaultOptions: { queries: { retry: false } },
              }),
            },
          ],
        ],
      },
    });
  });

  it('設定の切り替えはタスクの更新に require_all_assignees_done だけを送る', async () => {
    await completion.setRequireAll(task, true);

    expect(writes()).toHaveLength(1);
    expect(writes()[0]).toMatchObject({
      method: 'PUT',
      body: { require_all_assignees_done: true },
    });
    expect(writes()[0]!.url).toContain('/tenants/tenant-1/projects/project-1/tasks/task-1');
    expect(completion.errors.value['task-1']).toBeUndefined();
  });

  it('今と同じ値への切り替えは送らない', async () => {
    await completion.setRequireAll(task, false);

    expect(writes()).toHaveLength(0);
  });

  it('自分の分の完了と取り消しは担当者の complete / reopen を叩く', async () => {
    await completion.completeMine(task, 'user-alpha');
    await completion.reopenMine(task, 'user-alpha');

    expect(writes().map((w) => [w.method, new URL(w.url).pathname])).toEqual([
      [
        'POST',
        '/api/v1/tenants/tenant-1/projects/project-1/tasks/task-1/assignees/user-alpha/complete',
      ],
      [
        'POST',
        '/api/v1/tenants/tenant-1/projects/project-1/tasks/task-1/assignees/user-alpha/reopen',
      ],
    ]);
  });

  it.each([
    [409, 'task-already-done', 'タスクが完了済みです。ステータスを戻してから取り消してください'],
    [400, 'bad-request', '完了へ移せません。必須のカスタムフィールドを確認してください'],
    [403, 'forbidden', '更新に失敗しました'],
    [500, 'internal-error', '更新に失敗しました'],
  ])('%i（%s）の失敗は専用の文言を残す', async (status, message, text) => {
    control.fail = { status, message };

    await completion.completeMine(task, 'user-alpha');

    expect(completion.errors.value['task-1']).toBe(text);
    expect(completion.pending.value['task-1']).toBeUndefined();
  });

  it('次の操作を始めたら前のエラーを消す', async () => {
    control.fail = { status: 500, message: 'x' };
    await completion.completeMine(task, 'user-alpha');
    expect(completion.errors.value['task-1']).toBeDefined();

    control.fail = null;
    await completion.completeMine(task, 'user-alpha');
    await flushPromises();
    expect(completion.errors.value['task-1']).toBeUndefined();
  });
});

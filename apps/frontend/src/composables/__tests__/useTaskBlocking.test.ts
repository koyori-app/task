import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent } from 'vue';
import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import type { paths } from '@/generated/api';

import { useTaskBlocking } from '../useTaskBlocking';

const { control, requestLog, fetchMock } = vi.hoisted(() => {
  const control: {
    holdPost: boolean;
    releasePost: (() => void) | null;
    failStatus: number | null;
  } = { holdPost: false, releasePost: null, failStatus: null };
  const requestLog: { method: string; url: string; body?: unknown }[] = [];

  function response(body: unknown, status = 200) {
    return new Response(status === 204 ? null : JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  const base = {
    project_id: 'project-1',
    status_id: 'status-1',
    priority: 'Medium',
    progress_pct: 0,
    is_archived: false,
    assignees: [],
    labels: [],
    created_at: '2026-09-01T00:00:00Z',
    updated_at: '2026-09-01T00:00:00Z',
  };

  const fetchMock = async (request: Request) => {
    const entry: { method: string; url: string; body?: unknown } = {
      method: request.method,
      url: request.url,
    };
    if (request.method === 'POST') entry.body = await request.clone().json();
    requestLog.push(entry);

    if (request.method === 'GET' && request.url.includes('/relations')) {
      return response({
        subtasks: [],
        blocks: [
          { ...base, id: 'blocked-1', seq_id: 21, title: '止めている先', relation_id: 'rel-1' },
        ],
        blocked_by: [
          { ...base, id: 'blocker-1', seq_id: 22, title: '止めてくる元', relation_id: 'rel-2' },
        ],
      });
    }
    if (request.method === 'POST' || request.method === 'DELETE') {
      if (control.holdPost) {
        await new Promise<void>((resolve) => {
          control.releasePost = resolve;
        });
      }
      if (control.failStatus) return response({ message: 'failed' }, control.failStatus);
      return request.method === 'POST'
        ? response({ relation_id: 'rel-new' }, 201)
        : response(null, 204);
    }
    return response({});
  };

  return { control, requestLog, fetchMock };
});

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  const { default: createFetchClient } = await import('openapi-fetch');
  return {
    ...actual,
    fetchClient: createFetchClient<paths>({
      baseUrl: 'http://test.local/api',
      fetch: (request: Request) => fetchMock(request),
    }),
  };
});

describe('useTaskBlocking', () => {
  let queryClient: QueryClient;
  let blocking: ReturnType<typeof useTaskBlocking>;

  function mountHost() {
    const Host = defineComponent({
      setup() {
        blocking = useTaskBlocking({
          tenantId: 'tenant-1',
          projectId: 'project-1',
          taskId: 'TASK-11',
          taskUuid: 'task-1',
        });
        return () => null;
      },
    });
    return mount(Host, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
  }

  const posts = () => requestLog.filter((entry) => entry.method === 'POST');
  const relationGets = () =>
    requestLog.filter((entry) => entry.method === 'GET' && entry.url.includes('/relations'));

  beforeEach(() => {
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    control.holdPost = false;
    control.releasePost = null;
    control.failStatus = null;
    requestLog.length = 0;
  });

  it('blocks と blocked_by を relations API から取得する', async () => {
    mountHost();
    await flushPromises();

    expect(requestLog[0].url).toContain('/tasks/TASK-11/relations');
    expect(blocking.blocks.value.map((e) => [e.title, e.relation_id])).toEqual([
      ['止めている先', 'rel-1'],
    ]);
    expect(blocking.blockedBy.value.map((e) => [e.title, e.relation_id])).toEqual([
      ['止めてくる元', 'rel-2'],
    ]);
  });

  it('blocks で追加すると POST の body が { type: blocks, target_task_id } になり、relations と履歴を取り直す', async () => {
    mountHost();
    await flushPromises();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    const getsBefore = relationGets().length;

    await expect(blocking.addRelation('blocks', 'other-1')).resolves.toBe(true);

    expect(posts()[0].body).toEqual({ type: 'blocks', target_task_id: 'other-1' });
    expect(posts()[0].url).toContain('/tasks/TASK-11/relations');
    expect(relationGets().length).toBeGreaterThan(getsBefore);
    const keys = invalidate.mock.calls.map((call) => JSON.stringify(call[0]));
    expect(keys.some((key) => key.includes('/activities'))).toBe(true);
  });

  it('blocked_by で追加すると type が blocked_by になる', async () => {
    mountHost();
    await flushPromises();
    await expect(blocking.addRelation('blocked_by', 'other-2')).resolves.toBe(true);
    expect(posts()[0].body).toEqual({ type: 'blocked_by', target_task_id: 'other-2' });
  });

  it.each([
    [409, '循環する関係や同じ関係は設定できません'],
    [404, 'タスクが見つかりません。削除された可能性があります'],
    [503, 'ブロック関係を更新できませんでした'],
  ])('追加が %i なら文言を出し false を返す', async (status, message) => {
    mountHost();
    await flushPromises();
    control.failStatus = status;

    await expect(blocking.addRelation('blocks', 'other-1')).resolves.toBe(false);
    expect(blocking.mutationError.value).toBe(message);
    expect(blocking.mutating.value).toBe(false);
  });

  it('自分自身を相手にした追加は送らない', async () => {
    mountHost();
    await flushPromises();
    await expect(blocking.addRelation('blocks', 'task-1')).resolves.toBe(false);
    expect(posts()).toEqual([]);
  });

  it('飛行中の 2 件目は送らず false', async () => {
    mountHost();
    await flushPromises();
    control.holdPost = true;

    const first = blocking.addRelation('blocks', 'other-1');
    await flushPromises();
    expect(blocking.mutating.value).toBe(true);
    await expect(blocking.addRelation('blocked_by', 'other-2')).resolves.toBe(false);

    control.holdPost = false;
    control.releasePost?.();
    await expect(first).resolves.toBe(true);
    expect(posts()).toHaveLength(1);
  });

  it('removeRelation は DELETE /relations/{relation_id} を呼び、relations を取り直す', async () => {
    mountHost();
    await flushPromises();
    const getsBefore = relationGets().length;

    await expect(blocking.removeRelation('rel-1')).resolves.toBe(true);

    const del = requestLog.find((entry) => entry.method === 'DELETE');
    expect(del?.url).toContain('/tasks/TASK-11/relations/rel-1');
    expect(relationGets().length).toBeGreaterThan(getsBefore);
  });

  it('削除の失敗は文言を出し false で、relations は取り直さない', async () => {
    mountHost();
    await flushPromises();
    const getsBefore = relationGets().length;
    control.failStatus = 503;

    await expect(blocking.removeRelation('rel-1')).resolves.toBe(false);
    expect(blocking.mutationError.value).toBe('ブロック関係を外せませんでした');
    await flushPromises();
    expect(relationGets()).toHaveLength(getsBefore);
  });

  it('削除が 404（別の人が先に外した）なら実情に合う文言を出し、relations を取り直す', async () => {
    mountHost();
    await flushPromises();
    const getsBefore = relationGets().length;
    control.failStatus = 404;

    await expect(blocking.removeRelation('rel-1')).resolves.toBe(false);

    expect(blocking.mutationError.value).toBe('この関係は既に外されています');
    expect(relationGets().length).toBeGreaterThan(getsBefore);
    expect(blocking.mutating.value).toBe(false);
  });
});

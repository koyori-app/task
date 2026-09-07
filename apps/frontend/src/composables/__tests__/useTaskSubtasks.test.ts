import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, ref, type Ref } from 'vue';
import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import type { paths } from '@/generated/api';

import { useTaskSubtasks, type TaskSubtaskFilters } from '../useTaskSubtasks';

const { control, requestLog, fetchMock } = vi.hoisted(() => {
  const control: {
    holdPost: boolean;
    releasePost: (() => void) | null;
    rejectPost: boolean;
    rejectNextPage: boolean;
  } = { holdPost: false, releasePost: null, rejectPost: false, rejectNextPage: false };
  const requestLog: { method: string; url: string; body?: unknown }[] = [];

  function response(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  const parent = {
    id: 'parent-0',
    project_id: 'project-1',
    seq_id: 10,
    title: '上位タスク',
    status_id: 'status-1',
    priority: 'Medium',
    progress_pct: 0,
    is_archived: false,
    assignees: [],
    labels: [],
    created_at: '2026-09-01T00:00:00Z',
    updated_at: '2026-09-01T00:00:00Z',
  };
  const child = {
    ...parent,
    id: 'child-1',
    seq_id: 12,
    title: '既存の子',
    parent_task_id: 'parent-1',
  };

  const fetchMock = async (request: Request) => {
    const entry: { method: string; url: string; body?: unknown } = {
      method: request.method,
      url: request.url,
    };
    if (request.method === 'POST') entry.body = await request.clone().json();
    requestLog.push(entry);

    if (request.method === 'GET' && request.url.includes('/relations')) {
      return response({ parent, subtasks: [child], blocks: [], blocked_by: [] });
    }
    if (request.method === 'GET') {
      const query = new URL(request.url).searchParams;
      const nextPage = query.has('cursor');
      if (nextPage && control.rejectNextPage)
        return response({ message: 'temporary failure' }, 503);
      const label = query.get('label_id') ?? 'all';
      return response({
        tasks: Array.from({ length: nextPage ? 3 : 200 }, (_, i) => ({
          ...child,
          id: `${label}-${nextPage ? 2 - i : 202 - i}`,
        })),
        total: 203,
        next_cursor: nextPage ? null : 'next-page',
      });
    }
    if (request.method === 'POST') {
      if (control.holdPost) {
        await new Promise<void>((resolve) => {
          control.releasePost = resolve;
        });
      }
      if (control.rejectPost) return response({ message: 'temporary failure' }, 503);
      return response({ ...child, custom_field_values: [] }, 201);
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

describe('useTaskSubtasks', () => {
  let queryClient: QueryClient;
  let subtasks: ReturnType<typeof useTaskSubtasks>;

  function mountHost(filters?: Ref<TaskSubtaskFilters>, parentTaskId = ref<string | null>(null)) {
    const Host = defineComponent({
      setup() {
        subtasks = useTaskSubtasks({
          tenantId: 'tenant-1',
          projectId: 'project-1',
          taskId: 'TASK-11',
          taskUuid: 'parent-1',
          parentTaskId,
          filters,
        });
        return () => null;
      },
    });
    return mount(Host, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
  }

  beforeEach(() => {
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    control.holdPost = false;
    control.releasePost = null;
    control.rejectPost = false;
    control.rejectNextPage = false;
    requestLog.length = 0;
  });

  it('親と直下の子を relations API から取得する', async () => {
    mountHost();
    await flushPromises();

    expect(subtasks.parentTask.value?.title).toBe('上位タスク');
    expect(subtasks.subtasks.value.map((task) => task.title)).toEqual(['既存の子']);
    expect(requestLog[0].url).toContain('/tasks/TASK-11/relations');
  });

  it('親 UUID と継承ステータスを付けて作成し、relations を取り直す', async () => {
    mountHost();
    await flushPromises();
    const getCountBefore = requestLog.filter((entry) => entry.method === 'GET').length;

    await expect(subtasks.createSubtask('  新しい子  ', 'status-1')).resolves.toBe(true);

    const post = requestLog.find((entry) => entry.method === 'POST');
    expect(post?.body).toEqual({
      title: '新しい子',
      status_id: 'status-1',
      parent_task_id: 'parent-1',
    });
    expect(requestLog.filter((entry) => entry.method === 'GET').length).toBeGreaterThan(
      getCountBefore,
    );
  });

  it('送信中の二重作成を止める', async () => {
    mountHost();
    await flushPromises();
    control.holdPost = true;

    const first = subtasks.createSubtask('1件目', 'status-1');
    await flushPromises();
    await expect(subtasks.createSubtask('2件目', 'status-1')).resolves.toBe(false);

    control.holdPost = false;
    control.releasePost?.();
    await expect(first).resolves.toBe(true);
    expect(requestLog.filter((entry) => entry.method === 'POST')).toHaveLength(1);
  });

  it('親を持つタスクには孫を作らず、親IDの変更にも追従する', async () => {
    const parentTaskId = ref<string | null>(null);
    mountHost(undefined, parentTaskId);
    await flushPromises();
    parentTaskId.value = 'ancestor-id';
    await expect(subtasks.createSubtask('作れない孫', 'status-1')).resolves.toBe(false);
    expect(requestLog.filter((entry) => entry.method === 'POST')).toEqual([]);
    expect(subtasks.createPending.value).toBe(false);

    parentTaskId.value = null;
    await expect(subtasks.createSubtask('作れる子', 'status-1')).resolves.toBe(true);
    expect(requestLog.filter((entry) => entry.method === 'POST')).toHaveLength(1);
  });

  it('一時障害では false と再試行可能なエラーを返す', async () => {
    mountHost();
    await flushPromises();
    control.rejectPost = true;

    await expect(subtasks.createSubtask('失敗する子', 'status-1')).resolves.toBe(false);
    expect(subtasks.createError.value).toBe('サブタスクを作成できませんでした');
    expect(subtasks.createPending.value).toBe(false);
  });

  it('一覧条件を全ページへ渡し、条件変更で未絞り込みのキャッシュを混ぜない', async () => {
    mountHost();
    await flushPromises();
    const filters = ref<TaskSubtaskFilters>({
      label_id: 'X',
      status_id: 'todo',
      is_archived: false,
    });
    mountHost(filters);
    await flushPromises();
    expect(subtasks.subtasks.value).toHaveLength(203);
    expect(subtasks.subtasks.value[0].id).toBe('X-0');
    expect(subtasks.subtasks.value[202].id).toBe('X-202');
    const requests = requestLog.filter((r) => !r.url.includes('/relations'));
    expect(requests).toHaveLength(2);
    for (const request of requests) {
      const query = new URL(request.url).searchParams;
      expect(query.get('label_id')).toBe('X');
      expect(query.get('status_id')).toBe('todo');
      expect(query.get('is_archived')).toBe('false');
      expect(query.get('parent_task_id')).toBe('parent-1');
      expect(query.has('root_only')).toBe(false);
    }
    expect(new URL(requests[1].url).searchParams.get('cursor')).toBe('next-page');

    filters.value = { label_id: 'Y', status_id: 'done', is_archived: true };
    await flushPromises();
    expect(subtasks.subtasks.value[0].id).toBe('Y-0');
    const lastQuery = new URL(requestLog.at(-1)!.url).searchParams;
    expect(lastQuery.get('status_id')).toBe('done');
    expect(lastQuery.get('is_archived')).toBe('true');
  });

  it('後続ページの失敗を空の成功にせず、再試行で全件を取り直す', async () => {
    control.rejectNextPage = true;
    mountHost(ref({ is_archived: false }));
    await flushPromises();
    expect(subtasks.error.value).toBe(true);
    expect(subtasks.subtasks.value).toEqual([]);
    control.rejectNextPage = false;
    subtasks.refetch();
    await flushPromises();
    expect(subtasks.error.value).toBe(false);
    expect(subtasks.subtasks.value).toHaveLength(203);
  });
});

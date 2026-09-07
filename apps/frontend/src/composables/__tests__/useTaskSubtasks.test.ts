import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent } from 'vue';
import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import type { paths } from '@/generated/api';

import { useTaskSubtasks } from '../useTaskSubtasks';

const { control, requestLog, fetchMock } = vi.hoisted(() => {
  const control: {
    holdPost: boolean;
    releasePost: (() => void) | null;
    rejectPost: boolean;
  } = { holdPost: false, releasePost: null, rejectPost: false };
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

  function mountHost() {
    const Host = defineComponent({
      setup() {
        subtasks = useTaskSubtasks({
          tenantId: 'tenant-1',
          projectId: 'project-1',
          taskId: 'TASK-11',
          taskUuid: 'parent-1',
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

  it('一時障害では false と再試行可能なエラーを返す', async () => {
    mountHost();
    await flushPromises();
    control.rejectPost = true;

    await expect(subtasks.createSubtask('失敗する子', 'status-1')).resolves.toBe(false);
    expect(subtasks.createError.value).toBe('サブタスクを作成できませんでした');
    expect(subtasks.createPending.value).toBe(false);
  });
});

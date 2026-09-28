// @vitest-environment node
/**
 * SSR の釘。vike は server で renderToString するゆえ、DnD を足した
 * TaskGroupedList が document の無い node でそのまま描けることを直に撃つ。
 * （@formkit/drag-and-drop の import と、registerDndGroup が server では
 * 呼ばれぬこと——ref callback は client でしか火が入らぬ——の両方を釘にする）
 */

import { describe, expect, it } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';

import TaskGroupedList from '@/components/tasks/TaskGroupedList.vue';
import type { TaskGroup } from '@/components/tasks/task-grouped-columns';
import type { components } from '@/generated/api';

type StatusResponse = components['schemas']['ProjectStatusResponse'];
type TaskResponse = components['schemas']['TaskResponse'];

const status: StatusResponse = {
  id: 'status-todo',
  name: 'Todo',
  color: '#94a3b8',
  position: 0,
  is_default: true,
  is_done_state: false,
  is_default_done: false,
  project_id: 'project-1',
  created_at: '2026-06-01T00:00:00Z',
};

const task: TaskResponse = {
  id: 'task-1',
  project_id: 'project-1',
  seq_id: 1,
  title: 'SSR で描く行',
  description: null,
  status_id: status.id,
  priority: 'Medium',
  progress_pct: 0,
  soft_deadline: null,
  hard_deadline: null,
  is_archived: false,
  assignees: [],
  labels: [],
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-01T00:00:00Z',
};

const group: TaskGroup = {
  status,
  tasks: [task],
  total: 1,
  isLoading: false,
  isError: false,
  hasMore: false,
  oldestFirst: true,
  retry: () => {},
  loadMore: () => {},
};

describe('TaskGroupedList の SSR', () => {
  it('document の無い node で renderToString できる', async () => {
    expect(typeof document).toBe('undefined');
    const app = createSSRApp({
      render: () =>
        h(TaskGroupedList, {
          groups: [group],
          tenantId: 'tenant-1',
          projectId: 'project-1',
          projectKey: 'TASK',
          statuses: [status],
          projectLabels: [],
          members: [],
          pending: {},
          errors: {},
          sorting: [],
          onComment: async () => true,
          onCreate: async () => true,
        }),
    });
    const html = await renderToString(app);
    expect(html).toContain('SSR で描く行');
    expect(html).toContain('data-dnd-status-id="status-todo"');
  });
});

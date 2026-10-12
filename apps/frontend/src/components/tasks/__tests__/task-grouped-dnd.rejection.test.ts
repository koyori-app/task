/**
 * DnD の保存口（onMoveStatus）が reject したときの試験。
 *
 * lib が呼ぶ onTransfer を捕らえて直に撃つ（task-grouped-dnd.test.ts と同じ vikunja 式）。
 * gesture は jsdom 系では再現できぬので、dragAndDrop をすり替えて config を受け取る。
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

import TaskGroupedList from '@/components/tasks/TaskGroupedList.vue';
import type { TaskGroup } from '@/components/tasks/task-grouped-columns';
import type { components } from '@/generated/api';

type StatusResponse = components['schemas']['ProjectStatusResponse'];
type TaskResponse = components['schemas']['TaskResponse'];
type TransferHandler = (data: unknown) => void;

const transferHandlers = vi.hoisted(() => [] as TransferHandler[]);

vi.mock('@formkit/drag-and-drop/vue', () => ({
  dragAndDrop: (config: { onTransfer?: TransferHandler }) => {
    if (config.onTransfer) transferHandlers.push(config.onTransfer);
  },
}));

enableAutoUnmount(afterEach);
afterEach(() => {
  transferHandlers.length = 0;
});

function status(id: string, name: string, position: number): StatusResponse {
  return {
    id,
    name,
    color: '#94a3b8',
    position,
    is_default: position === 0,
    is_done_state: false,
    is_default_done: false,
    project_id: 'project-1',
    created_at: '2026-06-01T00:00:00Z',
  };
}

function task(id: string, statusId: string): TaskResponse {
  return {
    id,
    project_id: 'project-1',
    seq_id: 1,
    title: `task ${id}`,
    description: null,
    status_id: statusId,
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
}

function group(s: StatusResponse, tasks: TaskResponse[]): TaskGroup {
  return {
    status: s,
    tasks,
    total: tasks.length,
    isLoading: false,
    isError: false,
    hasMore: false,
    oldestFirst: true,
    retry: () => {},
    loadMore: () => {},
  };
}

describe('TaskGroupedList の DnD の保存口が reject したとき', () => {
  it('unhandled rejection を出さない', async () => {
    const unhandled: unknown[] = [];
    const onUnhandled = (reason: unknown) => unhandled.push(reason);
    process.on('unhandledRejection', onUnhandled);
    try {
      const todo = status('status-todo', 'Todo', 0);
      const done = status('status-done', 'Done', 1);
      const moved = task('task-1', todo.id);
      const onMoveStatus = vi.fn(async () => {
        throw new Error('save failed');
      });
      mount(TaskGroupedList, {
        props: {
          groups: [group(todo, [moved]), group(done, [])],
          tenantId: 'tenant-1',
          projectId: 'project-1',
          statuses: [todo, done],
          projectLabels: [],
          members: [],
          pending: {},
          errors: {},
          sorting: [],
          onComment: vi.fn(async () => true),
          onCreate: vi.fn(async () => true),
          onMoveStatus,
        },
        attachTo: document.body,
      });

      expect(transferHandlers.length).toBeGreaterThan(0);
      const target = document.createElement('div');
      target.dataset['dndStatusId'] = done.id;
      transferHandlers[0]!({
        targetParent: { el: target },
        draggedNodes: [{ data: { value: moved } }],
      });

      await flushPromises();
      await new Promise((resolve) => setTimeout(resolve, 20));

      expect(onMoveStatus).toHaveBeenCalledWith(moved, done.id);
      expect(unhandled).toEqual([]);
    } finally {
      process.off('unhandledRejection', onUnhandled);
    }
  });
});

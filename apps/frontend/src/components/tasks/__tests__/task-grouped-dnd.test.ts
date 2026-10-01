/**
 * 群間 DnD（spike）の試験。
 *
 * 撃ち方は vikunja 式に倣う——drag の gesture ではなく、lib が呼ぶ handler を
 * 直に撃ち、写った結果（update:status）を釘にする。gesture そのものは
 * jsdom では再現できぬ（DataTransfer / 座標判定が無い）。
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import TaskGroupedList from '@/components/tasks/TaskGroupedList.vue';
import { emitTransferAsStatusChange } from '@/components/tasks/task-grouped-dnd';
import type { TaskGroup } from '@/components/tasks/task-grouped-columns';
import type { components } from '@/generated/api';

enableAutoUnmount(afterEach);

type StatusResponse = components['schemas']['ProjectStatusResponse'];
type TaskResponse = components['schemas']['TaskResponse'];

function statusFixture(id: string, name: string, position = 0): StatusResponse {
  return {
    id,
    name,
    color: '#94a3b8',
    position,
    is_default: true,
    is_done_state: false,
    is_default_done: false,
    project_id: 'project-1',
    created_at: '2026-06-01T00:00:00Z',
  };
}

function taskFixture(id: string, statusId: string): TaskResponse {
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

function transferData(moved: TaskResponse, targetStatusId?: string) {
  const el = document.createElement('div');
  if (targetStatusId) el.dataset['dndStatusId'] = targetStatusId;
  return { targetParent: { el }, draggedNodes: [{ data: { value: moved } }] };
}

describe('emitTransferAsStatusChange（vikunja 式に handler を撃つ）', () => {
  it('群をまたぐ transfer を update:status へ写す', () => {
    const emitStatus = vi.fn();
    const moved = taskFixture('task-1', 'status-todo');
    emitTransferAsStatusChange(transferData(moved, 'status-done'), emitStatus);
    expect(emitStatus).toHaveBeenCalledTimes(1);
    expect(emitStatus).toHaveBeenCalledWith(moved, 'status-done');
  });

  it('同じ status への transfer は流さぬ', () => {
    const emitStatus = vi.fn();
    const moved = taskFixture('task-1', 'status-todo');
    emitTransferAsStatusChange(transferData(moved, 'status-todo'), emitStatus);
    expect(emitStatus).not.toHaveBeenCalled();
  });

  it('印（data-dnd-status-id）の無い container へは流さぬ', () => {
    const emitStatus = vi.fn();
    emitTransferAsStatusChange(transferData(taskFixture('task-1', 'status-todo')), emitStatus);
    expect(emitStatus).not.toHaveBeenCalled();
  });
});

function groupFixture(status: StatusResponse, tasks: TaskResponse[]): TaskGroup {
  return {
    status,
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

describe('TaskGroupedList の DnD の配線', () => {
  it('群ごとの container に status の印が付き、行が draggable の印を持つ', () => {
    const todo = statusFixture('status-todo', 'Todo');
    const done = statusFixture('status-done', 'Done', 1);
    const wrapper = mount(TaskGroupedList, {
      props: {
        groups: [groupFixture(todo, [taskFixture('task-1', todo.id)]), groupFixture(done, [])],
        tenantId: 'tenant-1',
        projectId: 'project-1',
        projectKey: 'TASK',
        statuses: [todo, done],
        projectLabels: [],
        members: [],
        pending: {},
        errors: {},
        sorting: [],
        onComment: vi.fn(async () => true),
        onCreate: vi.fn(async () => true),
        onMoveStatus: vi.fn(async () => undefined),
      },
      attachTo: document.body,
    });

    const containers = wrapper.findAll('[data-dnd-status-id]');
    expect(containers.map((c) => c.attributes('data-dnd-status-id'))).toEqual([
      'status-todo',
      'status-done',
    ]);
    expect(wrapper.findAll('[data-dnd-task]').length).toBe(1);
  });
});

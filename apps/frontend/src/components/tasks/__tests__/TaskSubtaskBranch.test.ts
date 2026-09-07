import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

const subtasksState = vi.hoisted(() => ({
  loading: { value: false },
  error: { value: false },
  subtasks: { value: [] as Array<{ id: string; title: string; status_id: string }> },
  createPending: { value: false },
  createError: { value: null as string | null },
  createSubtask: vi.fn(async () => true),
  refetch: vi.fn(),
}));

vi.mock('@/composables/useTaskSubtasks', () => ({
  useTaskSubtasks: () => subtasksState,
}));

import TaskSubtaskBranch from '@/components/tasks/TaskSubtaskBranch.vue';
import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';
import type { components } from '@/generated/api';

enableAutoUnmount(afterEach);

type TaskResponse = components['schemas']['TaskResponse'];

const parentTask = {
  id: 'parent-id',
  title: '親タスク',
  status_id: 'status-todo',
} as TaskResponse;

function mountBranch(pending: Record<string, 'status_id' | undefined>) {
  return mount(TaskSubtaskBranch, {
    props: {
      parentTask,
      tenantId: 'tenant-id',
      projectId: 'project-id',
      statuses: [],
      projectLabels: [],
      members: [],
      pending,
      errors: {},
      onComment: vi.fn(async () => true),
    },
    global: { stubs: { TaskGroupedRow: true } },
  });
}

describe('TaskSubtaskBranch', () => {
  beforeEach(() => {
    subtasksState.subtasks.value = [];
    subtasksState.createSubtask.mockClear();
  });

  it('親のステータス更新中は追加UIを無効化し、作成ハンドラも拒否する', async () => {
    const wrapper = mountBranch({});
    await wrapper.setProps({ pending: { [parentTask.id]: 'status_id' } });
    const composer = wrapper.findComponent(TaskSubtaskComposer);
    const onCreate = composer.props('onCreate') as (title: string) => Promise<boolean>;

    expect(composer.get('input').attributes('disabled')).toBeDefined();
    expect(await onCreate('競合する子')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });

  it('既存の子がある場合も親のステータス更新中は追加ボタンを無効化する', () => {
    subtasksState.subtasks.value = [
      { id: 'child-id', title: '子タスク', status_id: 'status-todo' },
    ];
    const wrapper = mountBranch({ [parentTask.id]: 'status_id' });
    const addButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'サブタスクを追加');

    expect(addButton).toBeDefined();
    expect(addButton!.attributes('disabled')).toBeDefined();
  });
});

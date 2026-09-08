import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { toValue } from 'vue';
import type { UseTaskSubtasksParams } from '@/composables/useTaskSubtasks';

const subtasksState = vi.hoisted(() => ({
  loading: { value: false },
  error: { value: false },
  subtasks: { value: [] as Array<{ id: string; title: string; status_id: string }> },
  createPending: { value: false },
  createError: { value: null as string | null },
  createSubtask: vi.fn(async () => true),
  refetch: vi.fn(),
  params: null as UseTaskSubtasksParams | null,
}));

vi.mock('@/composables/useTaskSubtasks', () => ({
  useTaskSubtasks: (params: UseTaskSubtasksParams) => {
    subtasksState.params = params;
    return subtasksState;
  },
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

  it('表示中の条件を取得処理へ渡し、展開中の条件変更にも追従する', async () => {
    const wrapper = mountBranch({});
    expect(toValue(subtasksState.params!.filters)).toEqual({ is_archived: false });
    await wrapper.setProps({ filters: { label_id: 'X', status_id: 'todo' } });
    expect(toValue(subtasksState.params!.filters)).toEqual({
      is_archived: false,
      label_id: 'X',
      status_id: 'todo',
    });
    await wrapper.setProps({ filters: { label_id: 'Y', status_id: 'done', is_archived: true } });
    expect(toValue(subtasksState.params!.filters)).toEqual({
      is_archived: true,
      label_id: 'Y',
      status_id: 'done',
    });
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

  it('ルートへ繰り上がった子には空の作成欄を出さず、送信も拒否する', async () => {
    const wrapper = mountBranch({});
    const onCreate = wrapper.getComponent(TaskSubtaskComposer).props('onCreate');
    await wrapper.setProps({ parentTask: { ...parentTask, parent_task_id: 'hidden-parent-id' } });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(toValue(subtasksState.params!.parentTaskId)).toBe('hidden-parent-id');
    expect(await onCreate('作れない孫')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });

  it('既存の子を持つサブタスクでも追加ボタンを出さない', async () => {
    subtasksState.subtasks.value = [
      { id: 'grandchild-id', title: '既存の孫', status_id: 'status-todo' },
    ];
    const wrapper = mountBranch({});
    await wrapper.setProps({ parentTask: { ...parentTask, parent_task_id: 'hidden-parent-id' } });
    expect(
      wrapper.findAll('button').some((button) => button.text().includes('サブタスクを追加')),
    ).toBe(false);
  });
});

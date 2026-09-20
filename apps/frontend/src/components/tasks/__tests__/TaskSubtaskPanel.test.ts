import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

const subtasksState = vi.hoisted(() => ({
  loading: { value: false },
  error: { value: false },
  subtasks: { value: [] as Array<{ id: string; title: string; status_id: string }> },
  parentTask: { value: null },
  createPending: { value: false },
  createError: { value: null as string | null },
  createSubtask: vi.fn(async () => true),
  refetch: vi.fn(),
}));

vi.mock('@/composables/useTaskSubtasks', () => ({
  useTaskSubtasks: () => subtasksState,
}));

import TaskSubtaskPanel from '@/components/tasks/TaskSubtaskPanel.vue';
import TaskSubtaskComposer from '@/components/tasks/TaskSubtaskComposer.vue';

enableAutoUnmount(afterEach);

function mountPanel(statusUpdating = false) {
  return mount(TaskSubtaskPanel, {
    props: {
      tenantId: 'tenant-id',
      projectId: 'project-id',
      taskId: 'TASK-1',
      taskUuid: 'task-id',
      parentTaskId: null,
      statusId: 'status-todo',
      statusUpdating,
      statuses: [],
      projectKey: 'TASK',
    },
  });
}

describe('TaskSubtaskPanel', () => {
  beforeEach(() => {
    subtasksState.subtasks.value = [];
    subtasksState.createSubtask.mockClear();
  });

  it('ステータス更新中は追加ボタンからComposerを開けない', async () => {
    const wrapper = mountPanel(true);
    const addButton = wrapper.get('button:not([aria-label])');

    expect(addButton.attributes('disabled')).toBeDefined();
    await addButton.trigger('click');
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
  });

  it('Composerを開いた後にステータス更新が始まっても作成を拒否する', async () => {
    const wrapper = mountPanel();
    await wrapper.get('button:not([aria-label])').trigger('click');
    await wrapper.setProps({ statusUpdating: true });

    const composer = wrapper.findComponent(TaskSubtaskComposer);
    const onCreate = composer.props('onCreate') as (title: string) => Promise<boolean>;
    expect(composer.get('input').attributes('disabled')).toBeDefined();
    expect(await onCreate('競合する子')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });

  it('親が取得できない子でも追加UIを出さず、既存の子への導線は残す', async () => {
    subtasksState.subtasks.value = [
      { id: 'grandchild-id', title: '既存の孫', status_id: 'status-todo' },
    ];
    const wrapper = mountPanel();
    await wrapper.setProps({ parentTaskId: 'deleted-parent-id' });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(
      wrapper.findAll('button').some((button) => button.text().includes('サブタスクを追加')),
    ).toBe(false);
    await wrapper.get('li button').trigger('click');
    expect(wrapper.emitted('open')?.[0]).toEqual([subtasksState.subtasks.value[0]]);
  });

  it('Composerを開いた後に子タスクへ変わったら入力欄を閉じ、保持された送信も拒否する', async () => {
    const wrapper = mountPanel();
    await wrapper.get('button:not([aria-label])').trigger('click');
    const onCreate = wrapper.getComponent(TaskSubtaskComposer).props('onCreate');
    await wrapper.setProps({ parentTaskId: 'parent-id' });
    expect(wrapper.findComponent(TaskSubtaskComposer).exists()).toBe(false);
    expect(await onCreate('作れない孫')).toBe(false);
    expect(subtasksState.createSubtask).not.toHaveBeenCalled();
  });
});

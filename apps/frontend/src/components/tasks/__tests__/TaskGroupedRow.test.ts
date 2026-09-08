import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import TaskGroupedRow from '@/components/tasks/TaskGroupedRow.vue';
import type { components } from '@/generated/api';

enableAutoUnmount(afterEach);

type TaskResponse = components['schemas']['TaskResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

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
  title: '親タスク',
  status_id: status.id,
  priority: 'Medium',
  progress_pct: 0,
  is_archived: false,
  assignees: [],
  labels: [],
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-01T00:00:00Z',
};

function mountRow(selected = false) {
  return mount(TaskGroupedRow, {
    props: {
      task,
      statuses: [status],
      projectLabels: [],
      members: [],
      selected,
      onComment: vi.fn(async () => true),
    },
  });
}

describe('TaskGroupedRow のサブタスク導線', () => {
  it('空白部分を選択した後だけ展開矢印を見せる', async () => {
    const wrapper = mountRow();
    const row = wrapper.get('[aria-label="タスク「親タスク」を選択"]');
    const toggle = wrapper.get('button[aria-label="サブタスクを展開"]');
    expect(toggle.classes()).toContain('opacity-0');

    await row.trigger('click');
    expect(wrapper.emitted('select')).toHaveLength(1);

    await wrapper.setProps({ selected: true });
    expect(toggle.classes()).toContain('opacity-100');
  });

  it('既存コントロールとタイトルのクリックは行選択にしない', async () => {
    const wrapper = mountRow();
    await wrapper.get('button[aria-label="ステータス: Todo"]').trigger('click');
    const title = wrapper.findAll('button').find((button) => button.text() === '親タスク');
    expect(title).toBeDefined();
    await title!.trigger('click');

    expect(wrapper.emitted('select')).toBeUndefined();
  });

  it('行へキーボードフォーカスを移すと選択でき、矢印は展開を通知する', async () => {
    const wrapper = mountRow(true);
    const row = wrapper.get('[aria-label="タスク「親タスク」を選択"]');
    await row.trigger('focus');
    expect(wrapper.emitted('select')).toHaveLength(1);

    await wrapper.get('button[aria-label="サブタスクを展開"]').trigger('click');
    expect(wrapper.emitted('toggle:subtasks')).toHaveLength(1);
  });

  it('子タスク行にはさらに展開する矢印を出さない', () => {
    const wrapper = mount(TaskGroupedRow, {
      props: {
        task,
        statuses: [status],
        projectLabels: [],
        members: [],
        showSubtaskToggle: false,
        depth: 1,
        onComment: vi.fn(async () => true),
      },
    });

    expect(wrapper.find('button[aria-label="サブタスクを展開"]').exists()).toBe(false);
  });
});

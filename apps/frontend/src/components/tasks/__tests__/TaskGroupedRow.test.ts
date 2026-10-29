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

describe('TaskGroupedRow の期限の表示', () => {
  const done: StatusResponse = {
    ...status,
    id: 'status-done',
    name: 'Done',
    is_default: false,
    is_done_state: true,
    is_default_done: true,
  };

  function mountWithDeadline(
    statusId: string,
    softDeadline: string,
    completedAt: string | null = null,
  ) {
    return mount(TaskGroupedRow, {
      props: {
        task: {
          ...task,
          status_id: statusId,
          soft_deadline: softDeadline,
          completed_at: completedAt,
        },
        statuses: [status, done],
        projectLabels: [],
        members: [],
        onComment: vi.fn(async () => true),
      },
    });
  }

  function deadlineButton(wrapper: ReturnType<typeof mountWithDeadline>, label: string) {
    const button = wrapper.findAll('button').find((b) => b.text() === label);
    expect(button, `期限の button「${label}」が無い`).toBeDefined();
    return button!;
  }

  afterEach(() => {
    vi.useRealTimers();
  });

  it('期限を過ぎてから完了した task は赤くせず、超過とも言わない', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-10T00:00:00');
    const button = deadlineButton(wrapper, '7月10日');
    expect(button.classes()).not.toContain('text-red-500');
    expect(wrapper.text()).not.toContain('超過');
  });

  it('期限の前に完了した task は、相対の字ではなく期限の日付を出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-16T00:00:00');
    const button = deadlineButton(wrapper, '7月16日');
    expect(button.classes()).not.toContain('text-red-500');
  });

  it('期限より後の日に完了した task は、遅れた日数を中立で添える', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-10T00:00:00', '2026-07-14T09:00:00');
    const button = deadlineButton(wrapper, '7月10日（4日超過）');
    expect(button.classes()).not.toContain('text-red-500');
  });

  it('期限の前に完了した task は、completed_at があっても期限の日付だけを出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-10T00:00:00', '2026-07-06T09:00:00');
    const button = deadlineButton(wrapper, '7月10日');
    expect(button.classes()).not.toContain('text-red-500');
  });

  it('completed_at が空の完了した task は、今日から数えず日付だけを出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-10T00:00:00', null);
    deadlineButton(wrapper, '7月10日');
    expect(wrapper.text()).not.toContain('超過');
  });

  it('完了していない task は今までどおり、過ぎた期限を赤く超過と出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00'));
    const wrapper = mountWithDeadline(status.id, '2026-07-10T00:00:00');
    const button = deadlineButton(wrapper, '4日超過');
    expect(button.classes()).toContain('text-red-500');
  });
});

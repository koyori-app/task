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
      projectKey: 'TEST',
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
        projectKey: 'TEST',
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

describe('TaskGroupedRow の優先度の表示', () => {
  it('優先度の色は旗だけに載せ、文字には載せない', () => {
    const wrapper = mountRow();

    expect(wrapper.html()).not.toContain('--priority-color');
    const flag = wrapper.get('button[aria-label="優先度"] svg');
    expect(flag.attributes('style')).toContain('color: #eab308');
  });

  // 文字が先だと「重大」と「中」で旗の位置が動く
  it('旗を文字より先に出す', () => {
    const html = mountRow().get('button[aria-label="優先度"]').html();

    expect(html.indexOf('<svg')).toBeGreaterThan(-1);
    expect(html.indexOf('<svg')).toBeLessThan(html.indexOf('中'));
    // 開閉の矢印を文字の後ろへ足さない
    expect(mountRow().get('button[aria-label="優先度"]').findAll('svg')).toHaveLength(1);
  });
});

describe('TaskGroupedRow のタイトルと補足', () => {
  const labels = [
    { id: 'label-1', name: 'UX改善', color: '#10b981' },
    { id: 'label-2', name: 'バグ', color: '#ef4444' },
    { id: 'label-3', name: 'フロントエンド', color: '#3b82f6' },
  ] as TaskResponse['labels'];

  function mountWith(overrides: Partial<TaskResponse>) {
    return mount(TaskGroupedRow, {
      props: {
        projectKey: 'TEST',
        task: { ...task, ...overrides },
        statuses: [status],
        projectLabels: [],
        members: [],
        onComment: vi.fn(async () => true),
      },
    });
  }

  // TASK-484: 同じ段に縮まないラベルを並べると、タイトルの幅が 0 になって読めず押せもしない
  it('タイトルは省略せず折り返し、ラベルと同じ段に並べない', () => {
    const wrapper = mountWith({ labels });
    const title = wrapper.get('button.text-left');

    expect(title.text()).toBe('親タスク');
    expect(title.classes()).toContain('break-words');
    expect(title.classes()).not.toContain('truncate');
    expect(wrapper.get('[data-row-labels]').text()).toContain('UX改善');
    expect(wrapper.get('[data-row-labels]').find('button.text-left').exists()).toBe(false);
    expect(title.element.parentElement?.contains(wrapper.get('[data-row-labels]').element)).toBe(
      true,
    );
    expect(title.element.nextElementSibling).toBe(wrapper.get('[data-row-labels]').element);
  });

  it('広い幅の 2 段目の ID を押してもコピーだけで、行を選択しない', async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const wrapper = mountWith({ seq_id: 7 });

    await wrapper.get('[data-row-labels] [data-task-id-copy]').trigger('click');

    expect(wrapper.emitted('select')).toBeUndefined();
    expect(wrapper.emitted('open')).toBeUndefined();
    expect(writeText).toHaveBeenCalledWith('TEST-7');
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('行の最小幅を持たず、列は一覧の幅が足りるときだけ並べる', () => {
    const grid = mountWith({}).get('[aria-label="タスク「親タスク」を選択"]');

    expect(grid.classes().some((name) => name.startsWith('min-w-'))).toBe(false);
    expect(grid.classes()).toContain('grid-cols-1');
  });

  it('狭い幅の 2 段目に ID と先頭のラベルと残りの件数を出す', () => {
    const summary = mountWith({ seq_id: 42, labels }).get('[data-row-summary]');

    expect(summary.text()).toContain('TEST-42');
    expect(summary.text()).toContain('UX改善');
    expect(summary.text()).toContain('+2');
    expect(summary.text()).not.toContain('バグ');
  });

  it('ラベルが 1 つなら件数を出さず、無ければ区切りも出さない', () => {
    expect(
      mountWith({ labels: labels.slice(0, 1) })
        .get('[data-row-summary]')
        .text(),
    ).not.toContain('+');
    expect(mountWith({ labels: [] }).get('[data-row-summary]').text()).not.toContain('·');
  });

  it('狭い幅の 2 段目で押せるのは ID のコピーだけで、スマホの幅では隠す', () => {
    const summary = mountWith({ labels, soft_deadline: '2026-06-20T00:00:00Z' }).get(
      '[data-row-summary]',
    );
    const controls = summary.findAll('button, a, input');

    expect(controls).toHaveLength(1);
    expect(controls[0]?.attributes('data-task-id-copy')).toBeDefined();
    expect(controls[0]?.classes()).toContain('max-[769px]:hidden');
  });

  it('狭い幅の開閉の矢印は丸の下に積み、選択しても丸とタイトルを動かさない', () => {
    const stack = mountWith({}).get('button[aria-label="サブタスクを展開"]').element.parentElement;

    expect(stack?.className).toContain('flex-col-reverse');
    expect(stack?.className).toContain('@3xl:flex-row');
    expect(stack?.querySelector('button[aria-label^="ステータス"]')).not.toBeNull();
  });

  // TASK-484: 「12月31日（123日超過）」のような長い期限で列が伸びると、狭い画面で ID に重なる
  it('狭い幅の期限の列は固定幅で、長い期限は省略する', () => {
    const deadline = mountWith({ soft_deadline: '2020-01-01T00:00:00Z' })
      .get('[data-row-summary]')
      .get('.text-destructive');

    expect(deadline.classes()).toEqual(
      expect.arrayContaining(['w-16', 'shrink-0', 'truncate', 'text-right']),
    );
    expect(deadline.classes()).not.toContain('min-w-11');
    expect(deadline.classes()).not.toContain('whitespace-nowrap');
  });

  it('狭い幅の 2 段目にも期限切れの色と担当者の名前を出す', () => {
    const summary = mountWith({
      soft_deadline: '2020-01-01T00:00:00Z',
      assignees: [
        { role: 'assignee', user: { id: 'u1', username: 'alice' } },
        { role: 'assignee', user: { id: 'u2', username: 'bob' } },
      ],
    }).get('[data-row-summary]');

    expect(summary.find('.text-destructive').exists()).toBe(true);
    expect(summary.text()).toContain('担当: alice、bob');
    expect(summary.text()).toContain('+1');
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
        projectKey: 'TEST',
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
    expect(button.classes()).not.toContain('text-destructive');
    expect(wrapper.text()).not.toContain('超過');
  });

  it('期限の前に完了した task は、相対の字ではなく期限の日付を出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-16T00:00:00');
    const button = deadlineButton(wrapper, '7月16日');
    expect(button.classes()).not.toContain('text-destructive');
  });

  it('期限より後の日に完了した task は、遅れた日数を中立で添える', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-10T00:00:00', '2026-07-14T09:00:00');
    const button = deadlineButton(wrapper, '7月10日（4日超過）');
    expect(button.classes()).not.toContain('text-destructive');
  });

  it('期限の前に完了した task は、completed_at があっても期限の日付だけを出す', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T12:00:00'));
    const wrapper = mountWithDeadline(done.id, '2026-07-10T00:00:00', '2026-07-06T09:00:00');
    const button = deadlineButton(wrapper, '7月10日');
    expect(button.classes()).not.toContain('text-destructive');
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
    expect(button.classes()).toContain('text-destructive');
  });
});

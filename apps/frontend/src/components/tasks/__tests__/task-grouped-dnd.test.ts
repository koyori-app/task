/**
 * 群間 DnD の試験。
 *
 * lib は差し替えず、行と群へ pointer・drag の event を直に撃ち、onMoveStatus へ届いた
 * 結果を釘にする。実 gesture は DOM だけの環境では再現できないので、WebKit を含む実
 * ブラウザでの確認は story（List 表示の DnD）と手元の Playwright で行う。
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { nextTick } from 'vue';

import TaskGroupedList from '@/components/tasks/TaskGroupedList.vue';
import { canStartRowDrag, statusIdAt } from '@/components/tasks/task-grouped-dnd';
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

describe('statusIdAt', () => {
  it('群の中の要素から status を引き、群の外は null', () => {
    const section = document.createElement('section');
    section.dataset['dndStatusId'] = 'status-done';
    const child = section.appendChild(document.createElement('p'));
    expect(statusIdAt(child)).toBe('status-done');
    expect(statusIdAt(document.createElement('p'))).toBeNull();
    expect(statusIdAt(null)).toBeNull();
  });
});

describe('canStartRowDrag', () => {
  it.each(['input', 'textarea', 'select'])('%s からは掴まない', (tag) => {
    expect(canStartRowDrag(document.createElement(tag))).toBe(false);
  });

  it('行の中に開いた一覧の項目からは掴まない', () => {
    const popover = document.createElement('div');
    popover.setAttribute('popover', 'manual');
    expect(canStartRowDrag(popover.appendChild(document.createElement('button')))).toBe(false);
  });

  it.each(['button', 'a', 'div'])('%s からは掴める', (tag) => {
    expect(canStartRowDrag(document.createElement(tag))).toBe(true);
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

const todo = statusFixture('status-todo', 'Todo');
const progress = statusFixture('status-progress', 'In Progress', 1);
const done = statusFixture('status-done', 'Done', 2);

/** Todo に行 2 つ、In Progress に行 1 つ、Done は空。 */
function mountList() {
  const moved = taskFixture('task-1', todo.id);
  // 保存中のままにする。確定すると鏡写しを正本（ここでは固定の props）から引き直すので、
  // 落とした時点で行が先に動くことを見られない
  const onMoveStatus = vi.fn(() => new Promise<void>(() => {}));
  const wrapper = mount(TaskGroupedList, {
    props: {
      groups: [
        groupFixture(todo, [moved, taskFixture('task-2', todo.id)]),
        groupFixture(progress, [taskFixture('task-3', progress.id)]),
        groupFixture(done, []),
      ],
      tenantId: 'tenant-1',
      projectId: 'project-1',
      statuses: [todo, progress, done],
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
  const section = (statusId: string) => wrapper.get(`[data-dnd-status-id="${statusId}"]`).element;
  const row = (taskId: string) =>
    wrapper.findAll('[data-dnd-task]').find((r) => r.text().includes(`task ${taskId}`))!
      .element as HTMLElement;
  return { wrapper, moved, onMoveStatus, section, row };
}

function fire(el: Element, type: string) {
  return el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true }));
}

function press(el: Element) {
  el.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, cancelable: true }));
}

describe('TaskGroupedList の DnD の配線', () => {
  it('群ごとの section に status の印が付き、行が draggable の印を持つ', () => {
    const { wrapper } = mountList();
    expect(
      wrapper.findAll('[data-dnd-status-id]').map((c) => c.attributes('data-dnd-status-id')),
    ).toEqual([todo.id, progress.id, done.id]);
    expect(wrapper.findAll('[data-dnd-task]').length).toBe(3);
  });
});

describe('TaskGroupedList の DnD: 掴む', () => {
  it('押した行にフォーカスが移っても掴めるまま（修正前は初回の押下で掴めなかった）', () => {
    const { row } = mountList();
    const rowEl = row('task-1');
    const focusable = rowEl.querySelector<HTMLElement>('[tabindex="0"]')!;
    press(focusable);
    focusable.focus();
    expect(rowEl.draggable).toBe(true);
    fire(rowEl, 'pointerup');
  });

  it('入力欄を押すと掴まず、次に行を押せばまた掴める', async () => {
    const { row, wrapper } = mountList();
    const rowEl = row('task-1');
    await wrapper.get('[aria-label="コメントを追加"]').trigger('click');
    const textarea = rowEl.querySelector('textarea')!;

    press(textarea);
    textarea.focus();
    expect(rowEl.draggable).toBe(false);

    press(rowEl.querySelector('[tabindex="0"]')!);
    expect(rowEl.draggable).toBe(true);
    fire(rowEl, 'pointerup');
  });
});

describe('TaskGroupedList の DnD: 落とす', () => {
  it.each([
    ['見出しのボタン', (s: Element) => s.querySelector('button')!],
    ['空の群の文言', (s: Element) => [...s.querySelectorAll('p')].at(-1)!],
    ['タスクを追加のボタン', (s: Element) => [...s.querySelectorAll('button')].at(-1)!],
    ['群そのもの（余白）', (s: Element) => s],
  ])('%s に落とすと、その群へ移す', async (_where, pick) => {
    const { moved, onMoveStatus, section, row } = mountList();
    fire(row('task-1'), 'dragstart');
    const target = pick(section(done.id));
    fire(target, 'dragover');
    fire(target, 'drop');
    await flushPromises();

    expect(onMoveStatus).toHaveBeenCalledTimes(1);
    expect(onMoveStatus).toHaveBeenCalledWith(moved, done.id);
    expect(section(done.id).textContent).toContain('task task-1');
    expect(section(todo.id).textContent).not.toContain('task task-1');
  });

  it('通っただけの群へは保存せず、行も動かさない', async () => {
    const { moved, onMoveStatus, section, row } = mountList();
    fire(row('task-1'), 'dragstart');
    fire(row('task-3'), 'dragenter');
    fire(row('task-3'), 'dragover');
    await flushPromises();

    expect(onMoveStatus).not.toHaveBeenCalled();
    expect(section(todo.id).textContent).toContain('task task-1');

    fire(section(done.id), 'dragover');
    fire(section(done.id), 'drop');
    await flushPromises();
    expect(onMoveStatus).toHaveBeenCalledTimes(1);
    expect(onMoveStatus).toHaveBeenCalledWith(moved, done.id);
  });

  it('要素へ入る dragenter を止める（止めないと WebKit は入った直後の drop を出さない）', () => {
    const { section, row } = mountList();
    const heading = section(done.id).querySelector('button')!;
    expect(fire(heading, 'dragenter')).toBe(true);

    fire(row('task-1'), 'dragstart');
    expect(fire(heading, 'dragenter')).toBe(false);
    fire(heading, 'drop');
  });

  it('元の群・群の外へ落としても、取り消しても保存しない', async () => {
    const { onMoveStatus, section, row } = mountList();

    fire(row('task-1'), 'dragstart');
    fire(section(todo.id), 'dragover');
    fire(section(todo.id), 'drop');
    await flushPromises();

    fire(row('task-1'), 'dragstart');
    fire(document.body, 'dragover');
    fire(document.body, 'drop');
    await flushPromises();

    fire(row('task-1'), 'dragstart');
    fire(section(done.id), 'dragover');
    fire(row('task-1'), 'dragend');
    await flushPromises();

    expect(onMoveStatus).not.toHaveBeenCalled();
    expect(section(todo.id).textContent).toContain('task task-1');
  });

  it('落とし先の群を強調し、元の群・群の外・終了後は外す', async () => {
    const { wrapper, section, row } = mountList();
    const highlighted = () =>
      wrapper
        .findAll('[data-dnd-status-id]')
        .filter((s) => s.classes('ring-2'))
        .map((s) => s.attributes('data-dnd-status-id'));

    fire(row('task-1'), 'dragstart');
    fire(section(done.id), 'dragover');
    await nextTick();
    expect(highlighted()).toEqual([done.id]);

    fire(section(todo.id), 'dragover');
    await nextTick();
    expect(highlighted()).toEqual([]);

    fire(section(done.id), 'dragover');
    fire(document.body, 'dragover');
    await nextTick();
    expect(highlighted()).toEqual([]);

    fire(section(done.id), 'dragover');
    await nextTick();
    expect(highlighted()).toEqual([done.id]);
    fire(row('task-1'), 'dragend');
    await nextTick();
    expect(highlighted()).toEqual([]);
  });
});

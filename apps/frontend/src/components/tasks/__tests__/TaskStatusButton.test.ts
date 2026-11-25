import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import type { components } from '@/generated/api';
import TaskStatusButton from '../TaskStatusButton.vue';

enableAutoUnmount(afterEach);

type Status = components['schemas']['ProjectStatusResponse'];

function status(id: string, position: number, overrides: Partial<Status> = {}): Status {
  return {
    id,
    project_id: 'project',
    name: id,
    color: '#3b82f6',
    position,
    is_default: false,
    is_done_state: false,
    is_default_done: false,
    created_at: '2026-01-01T00:00:00Z',
    ...overrides,
  } as Status;
}

const statuses = [
  status('Backlog', 0),
  status('In Progress', 1),
  // 既定でない完了を先に置く。✓ は position ではなく既定の完了へ移す
  status('Closed', 2, { is_done_state: true }),
  status('Done', 3, { is_done_state: true, is_default_done: true }),
];

function mountButton(props: Record<string, unknown> = {}) {
  return mount(TaskStatusButton, {
    props: { statuses, statusId: 'In Progress', ...props },
    attachTo: document.body,
  });
}

const doneButton = (wrapper: ReturnType<typeof mountButton>) =>
  wrapper.find('[data-task-status-done]');

describe('TaskStatusButton', () => {
  it('今のステータスの名前とアイコン（円の埋まり方）を出す', () => {
    const wrapper = mountButton();

    expect(wrapper.get('[data-task-status-name]').text()).toBe('In Progress');
    expect(wrapper.get('[data-task-status-icon]').attributes('data-fill')).toBe('partial');
    expect(
      mountButton({ statusId: 'Backlog' }).get('[data-task-status-icon]').attributes('data-fill'),
    ).toBe('empty');
  });

  it('長い名前は切る（幅の上限と truncate を持つ）', () => {
    const long = 'とても長いステータスの名前がここに入っていて一行には収まらない';
    const wrapper = mountButton({
      statuses: [status(long, 0), ...statuses],
      statusId: long,
    });

    const name = wrapper.get('[data-task-status-name]');
    expect(name.text()).toBe(long);
    expect(name.classes()).toContain('truncate');
    expect(name.element.closest('.max-w-48')).not.toBeNull();
  });

  it('✓ で既定の完了の id を emit する', async () => {
    const wrapper = mountButton();

    const done = doneButton(wrapper);
    expect(done.attributes('aria-label')).toBe('Done にする（完了）');
    await done.trigger('click');
    expect(wrapper.emitted('update:statusId')).toEqual([['Done']]);
  });

  it('既定の完了に印が無ければ、最初の完了扱いへ移す', async () => {
    const wrapper = mountButton({
      statuses: [status('Todo', 0), status('Closed', 1, { is_done_state: true })],
      statusId: 'Todo',
    });

    await doneButton(wrapper).trigger('click');
    expect(wrapper.emitted('update:statusId')).toEqual([['Closed']]);
  });

  it.each(['Done', 'Closed'])('完了済み（%s）なら ✓ を出さない', (statusId) => {
    expect(doneButton(mountButton({ statusId })).exists()).toBe(false);
  });

  it('完了扱いのステータスが無ければ ✓ を出さない', () => {
    const wrapper = mountButton({ statuses: [status('Todo', 0)], statusId: 'Todo' });
    expect(doneButton(wrapper).exists()).toBe(false);
  });

  it('完了条件を満たしていない間は、✓ を押せなくして理由を出す', async () => {
    const wrapper = mountButton({ doneBlocked: true, remainingAssignees: 2 });

    const done = doneButton(wrapper);
    // 隠さない。disabled にもしない（フォーカスと理由が届かなくなる）
    expect(done.exists()).toBe(true);
    expect(done.attributes('aria-disabled')).toBe('true');
    expect(done.attributes('disabled')).toBeUndefined();
    expect(wrapper.text()).toContain('担当者があと 2 人残っています');

    await done.trigger('click');
    expect(wrapper.emitted('update:statusId')).toBeUndefined();
  });

  it('完了条件を満たしていない間は、メニューの完了扱いの項目も選べない', () => {
    const wrapper = mountButton({ doneBlocked: true, remainingAssignees: 1 });

    const items = wrapper.findAll('[role="menuitemradio"]');
    const disabled = (name: string) =>
      items.find((item) => item.text() === name)!.attributes('aria-disabled') === 'true' ||
      items.find((item) => item.text() === name)!.attributes('disabled') !== undefined;
    expect(items.map((item) => item.text())).toEqual(['Backlog', 'In Progress', 'Closed', 'Done']);
    expect(disabled('Done')).toBe(true);
    expect(disabled('Closed')).toBe(true);
    expect(disabled('Backlog')).toBe(false);
  });

  it('メニューの今のステータスに ✓ が付く', () => {
    const wrapper = mountButton();
    const checked = wrapper
      .findAll('[role="menuitemradio"]')
      .filter((item) => item.attributes('aria-checked') === 'true');
    expect(checked.map((item) => item.text())).toEqual(['In Progress']);
  });

  it('メニューの各項目に、そのステータスの色と埋まり方のアイコンを出す', () => {
    const wrapper = mountButton({
      statuses: statuses.map((s) => (s.id === 'Done' ? { ...s, color: '#16a34a' } : s)),
    });
    const items = wrapper.findAll('[role="menuitemradio"]');
    const icons = items.map((item) => item.find('[data-task-status-icon]'));
    expect(icons.every((icon) => icon.exists())).toBe(true);
    expect(icons.map((icon) => icon.attributes('data-fill'))).toEqual([
      'empty',
      'partial',
      'done',
      'done',
    ]);
    expect(icons[3].html()).toContain('#16a34a');
  });

  it('「次のステータスへ」のボタンは無い', () => {
    const wrapper = mountButton();

    expect(wrapper.find('button[aria-label$="にする"]').exists()).toBe(false);
    expect(wrapper.html()).not.toContain('次へ');
    // ボタンはメニューのトリガーと ✓ の 2 つだけ（メニューの項目を除く）
    const buttons = wrapper
      .findAll('button')
      .filter((button) => !button.attributes('role')?.startsWith('menuitem'));
    expect(buttons).toHaveLength(2);
  });

  it('更新中は ✓ とメニューを止める', () => {
    const wrapper = mountButton({ updating: true });
    expect(doneButton(wrapper).attributes('disabled')).toBeDefined();
  });
});

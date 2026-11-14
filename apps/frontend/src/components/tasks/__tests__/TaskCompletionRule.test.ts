import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import type { components } from '@/generated/api';
import TaskCompletionRule from '../TaskCompletionRule.vue';

enableAutoUnmount(afterEach);

type Task = Pick<
  components['schemas']['TaskDetailResponse'],
  'require_all_assignees_done' | 'assignees'
>;

const alpha = { id: 'user-alpha', username: 'alpha', avatar_url: null };
const beta = { id: 'user-beta', username: 'beta', avatar_url: null };
const done = '2026-07-16T01:00:00Z';

function mountRule(task: Task, props: Record<string, unknown> = {}) {
  return mount(TaskCompletionRule, {
    props: { task, currentUserId: 'user-alpha', taskDone: false, ...props },
  });
}

function buttonLabels(wrapper: ReturnType<typeof mountRule>) {
  return wrapper.findAll('button').map((b) => b.text());
}

describe('TaskCompletionRule', () => {
  it('off のときはチェックだけを出し、担当者ごとの状態も自分用のボタンも出さない', () => {
    const wrapper = mountRule({
      require_all_assignees_done: false,
      assignees: [{ role: 'assignee', user: alpha, completed_at: null }],
    });

    const checkbox = wrapper.get('input[name="require-all-assignees-done"]');
    expect((checkbox.element as HTMLInputElement).checked).toBe(false);
    expect(wrapper.text()).toContain('担当者全員の完了が必要');
    expect(wrapper.find('[data-completion-progress]').exists()).toBe(false);
    expect(buttonLabels(wrapper)).not.toContain('自分の分を完了にする');
  });

  it('on のときは進み具合と担当者ごとの完了・未完了を出す', () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [
        { role: 'assignee', user: alpha, completed_at: null },
        { role: 'assignee', user: beta, completed_at: done },
      ],
    });

    expect(wrapper.get('[data-completion-progress]').text()).toBe('1 / 2 人が完了');
    const states = wrapper
      .findAll('[data-completion-assignees] li')
      .map((li) => [li.get('span').text(), li.get('[data-completed]').text()]);
    expect(states).toEqual([
      ['alpha', '未完了'],
      ['beta', '完了'],
    ]);
    expect(buttonLabels(wrapper)).toContain('自分の分を完了にする');
  });

  it('自分の分が未完了なら完了にするボタンが emit する', async () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [{ role: 'assignee', user: alpha, completed_at: null }],
    });

    await wrapper
      .findAll('button')
      .find((b) => b.text() === '自分の分を完了にする')!
      .trigger('click');
    expect(wrapper.emitted('complete')).toHaveLength(1);
  });

  it('自分の分が完了済みなら取り消すボタンを出し、押すと reopen を emit する', async () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [
        { role: 'assignee', user: alpha, completed_at: done },
        { role: 'assignee', user: beta, completed_at: null },
      ],
    });

    expect(buttonLabels(wrapper)).not.toContain('自分の分を完了にする');
    await wrapper
      .findAll('button')
      .find((b) => b.text() === '自分の完了を取り消す')!
      .trigger('click');
    expect(wrapper.emitted('reopen')).toHaveLength(1);
  });

  it('タスクが完了済みの間は取り消しを出さない', () => {
    const wrapper = mountRule(
      {
        require_all_assignees_done: true,
        assignees: [{ role: 'assignee', user: alpha, completed_at: done }],
      },
      { taskDone: true },
    );

    expect(buttonLabels(wrapper)).toEqual([]);
    expect(wrapper.get('[data-completion-progress]').text()).toBe('1 / 1 人が完了');
  });

  it('自分が担当者でなければ自分用のボタンを出さない', () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [{ role: 'assignee', user: beta, completed_at: null }],
    });

    expect(buttonLabels(wrapper)).toEqual([]);
  });

  it('担当者が 0 人なら案内を出す', () => {
    const wrapper = mountRule({ require_all_assignees_done: true, assignees: [] });

    expect(wrapper.text()).toContain('担当者を設定すると、全員の完了が必要になります');
    expect(wrapper.find('[data-completion-progress]').exists()).toBe(false);
  });

  it('チェックの切り替えを emit し、更新中は操作を止め、エラーを出す', async () => {
    const task: Task = { require_all_assignees_done: false, assignees: [] };
    const wrapper = mountRule(task);

    await wrapper.get('input[name="require-all-assignees-done"]').setValue(true);
    expect(wrapper.emitted('update:requireAll')).toEqual([[true]]);

    await wrapper.setProps({ updating: true, error: '更新に失敗しました' });
    expect(
      wrapper.get('input[name="require-all-assignees-done"]').attributes('disabled'),
    ).toBeDefined();
    expect(wrapper.get('[role="alert"]').text()).toBe('更新に失敗しました');
  });
});

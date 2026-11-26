import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import type { components } from '@/generated/api';
import TaskAssigneePicker from '../TaskAssigneePicker.vue';
import TaskCompletionRule from '../TaskCompletionRule.vue';

enableAutoUnmount(afterEach);

type Task = Pick<
  components['schemas']['TaskDetailResponse'],
  'require_all_assignees_done' | 'assignees'
>;

const user = (name: string) => ({ id: `user-${name}`, username: name, avatar_url: null });
const alpha = user('alpha');
const beta = user('beta');
const gamma = user('gamma');
const done = '2026-07-16T01:00:00Z';
const assignee = (u: ReturnType<typeof user>, completed = false) => ({
  role: 'assignee',
  user: u,
  completed_at: completed ? done : null,
});

function mountRule(task: Task, props: Record<string, unknown> = {}) {
  return mount(TaskCompletionRule, {
    props: { task, currentUserId: 'user-alpha', taskDone: false, ...props },
  });
}

const toggle = (wrapper: ReturnType<typeof mountRule>) => wrapper.get('[role="switch"]');
const button = (wrapper: ReturnType<typeof mountRule>, label: string) =>
  wrapper.find(`button[aria-label="${label}"]`);
/** 読み上げ専用の文字を除いた、見える文字 */
function visibleText(wrapper: ReturnType<typeof mountRule>) {
  const root = wrapper.element.cloneNode(true) as HTMLElement;
  for (const el of root.querySelectorAll('.sr-only')) el.remove();
  return root.textContent ?? '';
}

describe('TaskCompletionRule（担当者の欄）', () => {
  it('オフのときはスイッチと担当者のピッカーを出し、担当者ごとの状態は出さない', () => {
    const wrapper = mountRule(
      { require_all_assignees_done: false, assignees: [assignee(alpha)] },
      { members: [alpha, beta] },
    );

    expect(toggle(wrapper).attributes('aria-checked')).toBe('false');
    expect(wrapper.text()).toContain('全員の完了が必要');
    const pickers = wrapper.findAllComponents(TaskAssigneePicker);
    expect(pickers).toHaveLength(1);
    expect(pickers[0].props('trigger')).toBe('avatars');
    expect(wrapper.find('[data-completion-assignees]').exists()).toBe(false);
    expect(button(wrapper, '自分の分を完了にする').exists()).toBe(false);
  });

  it('オフでメンバーを渡されなければ、表示だけにする', () => {
    const wrapper = mountRule({
      require_all_assignees_done: false,
      assignees: [assignee(alpha)],
    });
    expect(wrapper.findComponent(TaskAssigneePicker).exists()).toBe(false);
    expect(wrapper.find('[aria-label="担当者"]').exists()).toBe(true);

    const empty = mountRule({ require_all_assignees_done: false, assignees: [] });
    expect(empty.text()).toContain('未設定');
  });

  it('オフでプロフィールの URL を渡されたら、担当者の名前をリンクで並べる', () => {
    const wrapper = mountRule(
      { require_all_assignees_done: false, assignees: [assignee(alpha), assignee(beta)] },
      { profileHref: (id: string) => `/acme/users/${id}` },
    );
    const links = wrapper.findAll('[data-assignee-profile-links] a');
    expect(links.map((a) => a.attributes('href'))).toEqual([
      '/acme/users/user-alpha',
      '/acme/users/user-beta',
    ]);
  });

  it('オンのときは自分の行を先頭に太字で出し、「まだ」を「済み」より上に並べる', () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      // 自分は 2 番目、済みの人が先に来る並び
      assignees: [assignee(beta, true), assignee(alpha), assignee(gamma)],
    });

    const mine = wrapper.get('[data-completion-mine]');
    expect(mine.text()).toContain('alpha（自分）');
    expect(mine.get('.font-semibold').text()).toBe('alpha（自分）');
    // 自分の行は一覧より前にある
    const container = wrapper.get('[data-completion-assignees]').element;
    expect(container.firstElementChild).toBe(mine.element);

    const others = wrapper.findAll('[data-completion-assignees] li');
    expect(
      others.map((li) => [
        li.get('.truncate').text(),
        li.get('.sr-only').text(),
        li.attributes('data-completed'),
      ]),
    ).toEqual([
      ['gamma', 'まだ', 'false'],
      ['beta', '済み', 'true'],
    ]);
    expect(toggle(wrapper).attributes('aria-checked')).toBe('true');
  });

  it('件数と「完了」「未完了」の文字は出さない（状態は読み上げ専用の文字で渡す）', () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [assignee(alpha), assignee(beta, true), assignee(gamma)],
    });

    const text = visibleText(wrapper);
    expect(text).not.toMatch(/\d\s*\/\s*\d/);
    expect(text).not.toContain('人が完了');
    expect(text).not.toContain('未完了');
    expect(text).not.toContain('済み');
    expect(text).not.toContain('まだ');
    expect(text).not.toContain('自分の分を完了にする');
    // 読み上げには状態が渡る
    expect(wrapper.findAll('.sr-only').map((el) => el.text())).toEqual(['まだ', '済み']);
  });

  it.each([
    [6, false],
    [7, true],
  ])('自分のほかが %i 人のとき、スクロールの器は %s', (count, scrollable) => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [
        assignee(alpha),
        ...Array.from({ length: count }, (_, i) => assignee(user(`member${i}`))),
      ],
    });

    expect(wrapper.find('[data-completion-scroll]').exists()).toBe(scrollable);
    expect(wrapper.findAll('[data-completion-assignees] li')).toHaveLength(count);
    // 自分の行はスクロールの器の外に固定する
    expect(wrapper.find('[data-completion-scroll] [data-completion-mine]').exists()).toBe(false);
    // スクロールしないときは、行が右端の 8px を自分で空ける（スクロールバーの幅と揃える）
    expect(wrapper.get('[data-completion-assignees] li').classes('pr-2')).toBe(!scrollable);
  });

  it('自分の分が未完了なら ✓ で complete を emit する', async () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [assignee(alpha)],
    });

    await button(wrapper, '自分の分を完了にする').trigger('click');
    expect(wrapper.emitted('complete')).toHaveLength(1);
  });

  it('自分の分が完了済みなら取り消せて、押すと reopen を emit する', async () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [assignee(alpha, true), assignee(beta)],
    });

    expect(button(wrapper, '自分の分を完了にする').exists()).toBe(false);
    await button(wrapper, '自分の完了を取り消す').trigger('click');
    expect(wrapper.emitted('reopen')).toHaveLength(1);
  });

  it('タスクが完了済みの間は取り消しを出さない', () => {
    const wrapper = mountRule(
      { require_all_assignees_done: true, assignees: [assignee(alpha, true)] },
      { taskDone: true },
    );

    expect(button(wrapper, '自分の完了を取り消す').exists()).toBe(false);
    expect(button(wrapper, '自分の分を完了にする').exists()).toBe(false);
    expect(wrapper.get('[data-completion-mine]').attributes('data-completed')).toBe('true');
  });

  it('自分が担当者でなければ自分の行もボタンも出さない', () => {
    const wrapper = mountRule({
      require_all_assignees_done: true,
      assignees: [assignee(beta)],
    });

    expect(wrapper.find('[data-completion-mine]').exists()).toBe(false);
    expect(button(wrapper, '自分の分を完了にする').exists()).toBe(false);
    expect(wrapper.findAll('[data-completion-assignees] li')).toHaveLength(1);
  });

  it('担当者が 0 人でオンなら案内を出す', () => {
    const wrapper = mountRule({ require_all_assignees_done: true, assignees: [] });

    expect(wrapper.text()).toContain('担当者を設定すると、全員の完了が必要になります');
    expect(wrapper.find('[data-completion-assignees]').exists()).toBe(false);
  });

  it('オンのときは、スイッチの行に担当者を編集する入口を置く', async () => {
    const wrapper = mountRule(
      { require_all_assignees_done: true, assignees: [assignee(alpha)] },
      { members: [alpha, beta], assigneeUpdating: true },
    );

    const picker = wrapper.getComponent(TaskAssigneePicker);
    expect(picker.props('trigger')).toBe('icon');
    expect(picker.props('disabled')).toBe(true);
    picker.vm.$emit('toggle', 'user-beta', true);
    expect(wrapper.emitted('toggle:assignee')).toEqual([['user-beta', true]]);

    // メンバーを渡されなければ入口は出さない
    const readonly = mountRule({ require_all_assignees_done: true, assignees: [assignee(alpha)] });
    expect(readonly.findComponent(TaskAssigneePicker).exists()).toBe(false);
  });

  it('スイッチの切り替えを emit し、更新中は操作を止め、エラーを出す', async () => {
    const task: Task = { require_all_assignees_done: false, assignees: [] };
    const wrapper = mountRule(task);

    await toggle(wrapper).trigger('click');
    expect(wrapper.emitted('update:requireAll')).toEqual([[true]]);

    await wrapper.setProps({ updating: true, error: '更新に失敗しました' });
    expect(toggle(wrapper).attributes('disabled')).toBeDefined();
    expect(wrapper.get('[role="alert"]').text()).toBe('更新に失敗しました');
  });
});

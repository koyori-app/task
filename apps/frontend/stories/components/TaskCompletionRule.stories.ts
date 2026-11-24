import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';

import TaskCompletionRule from '@/components/tasks/TaskCompletionRule.vue';

const user = (id: string, username: string) => ({ id, username, avatar_url: null });
const me = user('user-me', 'yupix');
const tanaka = user('user-tanaka', '田中太郎');
const done = '2026-07-14T03:00:00Z';
const assignee = (u: ReturnType<typeof user>, completed = false) => ({
  role: 'assignee',
  user: u,
  completed_at: completed ? done : null,
});
/** 多人数の担当者。奇数番目は済み */
const crowd = Array.from({ length: 9 }, (_, i) =>
  assignee(user(`user-${i}`, `メンバー${i + 1}`), i % 2 === 1),
);

const onComplete = fn();
const onUpdateRequireAll = fn();

const meta = {
  title: 'Components/Tasks/TaskCompletionRule',
  component: TaskCompletionRule,
  tags: ['autodocs'],
  args: {
    currentUserId: me.id,
    taskDone: false,
    members: [me, tanaka, ...crowd.map((a) => a.user)],
    profileHref: (id: string) => `/acme/users/${id}`,
    // emit の受け口は props の型に無いので、spy は外に持って渡す
    onComplete,
    'onUpdate:requireAll': onUpdateRequireAll,
  } as never,
  beforeEach: () => {
    onComplete.mockClear();
    onUpdateRequireAll.mockClear();
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'タスク詳細の担当者の欄。完了条件（全員の完了が必要）がオフのときは重ねたアバターとスイッチ、オンのときは 1 人 1 行の縦リスト（自分の行を先頭に固定、「まだ」を「済み」より上）。状態は色とアイコンで出し、文字と件数は重ねない。',
      },
    },
  },
  decorators: [() => ({ template: '<div class="w-64 text-sm"><story /></div>' })],
} satisfies Meta<typeof TaskCompletionRule>;

export default meta;
type Story = StoryObj<typeof meta>;

const rows = (canvasElement: HTMLElement) => [
  ...canvasElement.querySelectorAll<HTMLElement>(
    '[data-completion-mine], [data-completion-assignees] li',
  ),
];

export const Off: Story = {
  name: 'オフ（重ねたアバターとスイッチ）',
  args: {
    task: { require_all_assignees_done: false, assignees: [assignee(me), assignee(tanaka)] },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = canvas.getByRole('switch', { name: '全員の完了が必要' });
    await expect(toggle).not.toBeChecked();
    await expect(canvasElement.querySelector('[data-completion-assignees]')).toBeNull();
    await expect(canvas.getByRole('link', { name: '田中太郎' })).toHaveAttribute(
      'href',
      '/acme/users/user-tanaka',
    );
    await userEvent.click(toggle);
    await expect(onUpdateRequireAll).toHaveBeenCalledWith(true);
  },
};

export const OnTwo: Story = {
  name: 'オン 2 人（自分の行が先頭）',
  args: {
    task: { require_all_assignees_done: true, assignees: [assignee(tanaka, true), assignee(me)] },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const [mine, other] = rows(canvasElement);
    await expect(mine).toHaveAttribute('data-completion-mine');
    await expect(within(mine).getByText('yupix（自分）')).toHaveClass('font-semibold');
    await expect(other).toHaveAttribute('data-completed', 'true');
    // 状態の文字は読み上げ専用。件数は出さない
    await expect(within(other).getByText('済み')).toHaveClass('sr-only');
    await expect(canvas.queryByText(/\d\s*\/\s*\d/)).toBeNull();
    // 自分の ✓ は 24px の枠つきのボタンで、右端の状態の列（幅 24px）とそろう
    const complete = canvas.getByRole('button', { name: '自分の分を完了にする' });
    const button = complete.getBoundingClientRect();
    await expect([button.width, button.height]).toEqual([24, 24]);
    const state = other.lastElementChild!.getBoundingClientRect();
    await expect(state.width).toBe(24);
    await expect(state.left).toBe(button.left);
    // 行の高さは 28px（既定の書体のまま測る）
    for (const row of [mine, other]) await expect(row.getBoundingClientRect().height).toBe(28);
    await expect(canvas.getByRole('switch', { name: '全員の完了が必要' })).toBeChecked();
    // 縦リストにはアバターの束が無いので、スイッチの行に担当者を編集する入口を置く
    await expect(canvas.getByRole('button', { name: /^担当者を編集/ })).toBeVisible();

    await userEvent.click(complete);
    await expect(onComplete).toHaveBeenCalled();
  },
};

export const OnMany: Story = {
  name: 'オン多人数（6 行を超えたら自分の行の下だけスクロール）',
  args: {
    task: {
      require_all_assignees_done: true,
      assignees: [...crowd.slice(0, 4), assignee(me, true), ...crowd.slice(4)],
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const list = canvasElement.querySelector<HTMLElement>('[data-completion-scroll]')!;
    // 自分の行はスクロールの器の外に固定する
    const mine = canvasElement.querySelector<HTMLElement>('[data-completion-mine]')!;
    await expect(list.contains(mine)).toBe(false);
    await expect(mine.getBoundingClientRect().bottom).toBeLessThanOrEqual(
      list.getBoundingClientRect().top,
    );
    // 6 行（168px）で止まり、その中だけスクロールする
    await expect(list.getBoundingClientRect().height).toBe(168);
    await expect(list.scrollHeight).toBeGreaterThan(list.clientHeight);
    // 「まだ」を上、「済み」を下に並べる
    const states = [...list.querySelectorAll('li')].map((li) => li.getAttribute('data-completed'));
    await expect(states).toEqual([...states].sort((a, b) => (a ?? '').localeCompare(b ?? '')));
    // スクロールバーの幅（8px）を空け、右端の状態の列を自分の行とそろえる
    const stateLeft = (row: HTMLElement) => row.lastElementChild!.getBoundingClientRect().left;
    await expect(stateLeft(list.querySelector('li')!)).toBe(stateLeft(mine));
    await expect(list.offsetWidth - list.clientWidth).toBe(8);
    // 下端はぼかす
    await expect(getComputedStyle(list).maskImage).toContain('linear-gradient');
    // 自分は完了済み。取り消せる
    await expect(canvas.getByRole('button', { name: '自分の完了を取り消す' })).toBeVisible();
  },
};

export const OnTaskDone: Story = {
  name: 'オン（タスクが完了済みの間は取り消しを出さない）',
  args: {
    taskDone: true,
    task: {
      require_all_assignees_done: true,
      assignees: [assignee(me, true), assignee(tanaka, true)],
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.queryByRole('button', { name: '自分の完了を取り消す' })).toBeNull();
    await expect(canvas.queryByRole('button', { name: '自分の分を完了にする' })).toBeNull();
  },
};

export const OnWithoutAssignees: Story = {
  name: 'オンで担当者 0 人（案内）',
  args: { task: { require_all_assignees_done: true, assignees: [] } },
  play: async ({ canvasElement }) => {
    await expect(
      within(canvasElement).getByText('担当者を設定すると、全員の完了が必要になります'),
    ).toBeVisible();
  },
};

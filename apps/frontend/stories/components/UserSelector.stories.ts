import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ref } from 'vue';

import TaskAssigneePicker from '@/components/tasks/TaskAssigneePicker.vue';
import type { SelectableUser } from '@/components/users/types';
import UserSelectorList from '@/components/users/UserSelectorList.vue';

const NAMES = [
  'yupix',
  'sousuke',
  'kousuke',
  'alice',
  'bob',
  'carol',
  'dave',
  'eve',
  'frank',
  'grace',
  'ユウキ',
  'さくら',
];

function people(count: number): SelectableUser[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `user-${i + 1}`,
    username: NAMES[i] ?? `member${String(i + 1).padStart(3, '0')}`,
    avatar_url: null,
  }));
}

/** 担当者の付け外しを手元で持つ（一人ずつ保存する呼び手と同じ `toggle` を受ける）。 */
function pickerStory(members: SelectableUser[], initial: string[], currentUserId?: string) {
  return {
    components: { TaskAssigneePicker },
    setup() {
      const selectedIds = ref<string[]>(initial);
      const onToggle = (id: string, checked: boolean) => {
        selectedIds.value = checked
          ? [...selectedIds.value, id]
          : selectedIds.value.filter((x) => x !== id);
      };
      const selected = () => members.filter((m) => selectedIds.value.includes(m.id));
      return { members, selected, onToggle, currentUserId };
    },
    template: `<div class="p-4">
      <TaskAssigneePicker
        :members="members"
        :selected="selected()"
        :current-user-id="currentUserId"
        @toggle="onToggle"
      />
    </div>`,
  };
}

const meta = {
  title: 'Components/UserSelector',
  component: UserSelectorList,
  args: { users: people(5), modelValue: [] },
} satisfies Meta<typeof UserSelectorList>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 選んだ者が先、次に自分。開いている間は付け外ししても並びは動かない。 */
export const Default: Story = {
  render: () => pickerStory(people(12), ['user-2', 'user-1'], 'user-9'),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);
    const user = userEvent.setup();

    const trigger = canvas.getByRole('button', { name: /^担当者: / });
    await user.click(trigger);
    const listbox = await body.findByRole('listbox');
    const names = () =>
      within(listbox)
        .getAllByRole('option')
        .map((o) => o.getAttribute('data-user-id'));

    // 選んだ者（名の順）、自分、その他
    await expect(names().slice(0, 3)).toEqual(['user-2', 'user-1', 'user-9']);
    const before = names();

    // 絞り込んでキーボードで選ぶ
    const input = body.getByRole('textbox', { name: 'メンバーを検索' });
    await expect(input).toHaveFocus();
    await user.type(input, 'ali');
    await expect(names()).toEqual(['user-4']);
    await user.keyboard('{ArrowDown}{Enter}');
    await waitFor(() =>
      expect(within(listbox).getByRole('option', { selected: true })).toHaveAttribute(
        'data-user-id',
        'user-4',
      ),
    );

    // 絞り込みを消しても、選んだ者は上へ飛ばない（開いている間は並びを保つ）
    await user.clear(input);
    await expect(names()).toEqual(before);

    // Esc で閉じて trigger へ戻り、開き直すと選んだ者が先に来る
    await user.keyboard('{Escape}');
    await waitFor(() => expect(body.queryByRole('listbox')).not.toBeInTheDocument());
    await expect(trigger).toHaveFocus();
    await user.click(canvas.getByRole('button', { name: /^担当者: / }));
    const reopened = await body.findByRole('listbox');
    await expect(
      within(reopened)
        .getAllByRole('option')
        .slice(0, 3)
        .map((o) => o.getAttribute('data-user-id')),
    ).toEqual(['user-4', 'user-2', 'user-1']);
    await user.keyboard('{Escape}');
  },
};

/** 多くの人数（500 人）。描くのは先頭の 100 人までで、残りは数を添えて名での絞り込みを促す。 */
export const ManyUsers: Story = {
  render: () => pickerStory(people(500), []),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);
    const user = userEvent.setup();

    await user.click(canvas.getByRole('button', { name: '担当者を割り当てる' }));
    const listbox = await body.findByRole('listbox');
    await expect(within(listbox).getAllByRole('option')).toHaveLength(100);
    await expect(body.getByText('ほか 400 人。名で絞り込めます')).toBeInTheDocument();

    await user.type(body.getByRole('textbox', { name: 'メンバーを検索' }), 'member499');
    await expect(within(listbox).getAllByRole('option')).toHaveLength(1);
    await user.keyboard('{Escape}');
  },
};

/** 選んだ者の重ね: 3 人まで重ね、超えた分は +N。名は aria-label で全員分読める。 */
export const StackedSelection: Story = {
  render: () => pickerStory(people(8), ['user-1', 'user-2', 'user-3', 'user-4', 'user-5']),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const trigger = canvas.getByRole('button', {
      name: '担当者: yupix、sousuke、kousuke、alice、bob',
    });
    await expect(within(trigger).getByText('+2')).toBeInTheDocument();
  },
};

/** モバイルの幅（320px の枠）。窓は画面に収まり、項は触れる的として足りる高さを持つ。 */
export const NarrowWidth: Story = {
  render: () => ({
    components: { TaskAssigneePicker },
    setup: () => ({ members: people(6) }),
    template: `<div style="width: 320px" class="flex justify-end border p-2">
      <TaskAssigneePicker :members="members" :selected="[]" />
    </div>`,
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);
    const user = userEvent.setup();

    await user.click(canvas.getByRole('button', { name: '担当者を割り当てる' }));
    const listbox = await body.findByRole('listbox');
    const content = listbox.closest('[data-slot="user-selector-content"]') as HTMLElement;
    const rect = content.getBoundingClientRect();
    await expect(rect.left).toBeGreaterThanOrEqual(0);
    await expect(rect.right).toBeLessThanOrEqual(window.innerWidth);
    for (const option of within(listbox).getAllByRole('option')) {
      // WCAG 2.2 の Target Size（最小）: 24 CSS px
      await expect(option.getBoundingClientRect().height).toBeGreaterThanOrEqual(24);
    }
    await user.keyboard('{Escape}');
  },
};

/** 取得中・失敗・0 人・一致なしの見え方（窓の中身を並べて見せる）。 */
export const States: Story = {
  render: () => ({
    components: { UserSelectorList },
    setup: () => ({ users: people(3), retry: () => {} }),
    template: `<div class="grid w-[34rem] grid-cols-2 gap-3">
      <div class="rounded-md border"><UserSelectorList :users="[]" :model-value="[]" :state="{ loading: true }" /></div>
      <div class="rounded-md border"><UserSelectorList :users="[]" :model-value="[]" :state="{ error: true, onRetry: retry }" /></div>
      <div class="rounded-md border"><UserSelectorList :users="[]" :model-value="[]" /></div>
      <div class="rounded-md border"><UserSelectorList :users="users" :model-value="['user-2']" current-user-id="user-3" /></div>
    </div>`,
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('読み込み中…')).toBeInTheDocument();
    await expect(canvas.getByText('候補を読み込めませんでした')).toBeInTheDocument();
    await expect(canvas.getByText('選べる利用者がいません')).toBeInTheDocument();
    await expect(canvas.getByRole('option', { selected: true })).toHaveAttribute(
      'data-user-id',
      'user-2',
    );
  },
};

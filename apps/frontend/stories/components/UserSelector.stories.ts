import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ref } from 'vue';

import TaskAssigneePicker from '@/components/tasks/TaskAssigneePicker.vue';
import type { SelectableUser } from '@/components/users/types';
import UserSelectorList from '@/components/users/UserSelectorList.vue';
import { orderUsersForPicker } from '@/lib/user-search';

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

/**
 * 多くの人数。候補は仮想化して全員を描く（DOM に載るのは見えている項だけ）。開くまでの時間と
 * 絞り込みの応えを測り、キーボードの End で末尾の者まで一覧が送られて届くことを見る。
 * 測った値は console に `MEASURE` として出す（閾値は緩く置き、遅さの退行だけを捕まえる）。
 */
async function measureManyUsers(canvasElement: HTMLElement, count: number) {
  const canvas = within(canvasElement);
  const body = within(document.body);
  const user = userEvent.setup();
  const members = people(count);
  // 末尾は並べた後の最後の者（名の順。片仮名の名が後ろに来る）
  const last = orderUsersForPicker(members, [], null).at(-1)!.id;
  const deepest = `user-${count}`;

  const t0 = performance.now();
  await user.click(canvas.getByRole('button', { name: '担当者を割り当てる' }));
  const listbox = await body.findByRole('listbox');
  await waitFor(() => expect(within(listbox).getAllByRole('option').length).toBeGreaterThan(0));
  const openMs = performance.now() - t0;

  const input = body.getByRole('textbox', { name: 'メンバーを検索' });
  const query = `member${String(count).padStart(3, '0')}`;
  const t1 = performance.now();
  await user.type(input, query);
  await waitFor(() =>
    expect(
      within(listbox)
        .getAllByRole('option')
        .map((o) => o.getAttribute('data-user-id')),
    ).toEqual([deepest]),
  );
  const filterMs = performance.now() - t1;

  await user.clear(input);
  await expect(
    within(listbox).queryByRole('option', { name: members.at(-1)!.username }),
  ).toBeNull();
  await user.keyboard('{End}');
  await waitFor(async () => {
    const highlighted = listbox.querySelector('[data-highlighted]');
    await expect(highlighted?.getAttribute('data-user-id')).toBe(last);
    // 末尾の項が一覧の見える範囲に送られておる
    const box = listbox.getBoundingClientRect();
    const rect = highlighted!.getBoundingClientRect();
    await expect(rect.top).toBeGreaterThanOrEqual(box.top - 1);
    await expect(rect.bottom).toBeLessThanOrEqual(box.bottom + 1);
    // 支援技術へも、照らしておる項が伝わっておる
    await expect(input.getAttribute('aria-activedescendant')).toBe(highlighted!.id);
  });
  await expect(within(listbox).getAllByRole('option').length).toBeLessThan(60);

  console.log(
    `MEASURE users=${count} open=${openMs.toFixed(0)}ms filter(${query.length} keys)=${filterMs.toFixed(0)}ms per-key=${(filterMs / query.length).toFixed(1)}ms`,
  );
  await expect(openMs).toBeLessThan(3000);
  await user.keyboard('{Escape}');
}

export const ManyUsers: Story = {
  render: () => pickerStory(people(500), []),
  play: async ({ canvasElement }) => measureManyUsers(canvasElement, 500),
};

export const HugeUsers: Story = {
  render: () => pickerStory(people(5000), []),
  play: async ({ canvasElement }) => measureManyUsers(canvasElement, 5000),
};

/**
 * 保存が遅れて反映される呼び手（担当者は一人ずつ POST して再取得し、保存中は選択を止める）。
 * 選んだ後に `modelValue` が遅れて変わっても、一覧は送った位置のまま動かず、後ろの候補を
 * 続けて選べる。
 */
export const DeferredSave: Story = {
  render: () => {
    const members = people(40);
    return {
      components: { TaskAssigneePicker },
      setup() {
        const selectedIds = ref<string[]>(['user-1']);
        const saving = ref(false);
        const onToggle = (id: string, checked: boolean) => {
          // 保存と再取得を待ってから反映し、その間は選択を止める
          saving.value = true;
          setTimeout(() => {
            selectedIds.value = checked
              ? [...selectedIds.value, id]
              : selectedIds.value.filter((x) => x !== id);
            saving.value = false;
          }, 80);
        };
        const selected = () => members.filter((m) => selectedIds.value.includes(m.id));
        return { members, selected, saving, onToggle };
      },
      template: `<div class="p-4">
        <TaskAssigneePicker
          :members="members"
          :selected="selected()"
          :disabled="saving"
          @toggle="onToggle"
        />
      </div>`,
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);
    const user = userEvent.setup();

    await user.click(canvas.getByRole('button', { name: /^担当者: / }));
    const listbox = await body.findByRole('listbox');
    await waitFor(() => expect(within(listbox).getAllByRole('option').length).toBeGreaterThan(0));

    // 一覧を下へ送り、見えておる後ろの候補を選ぶ
    listbox.scrollTop = listbox.scrollHeight;
    await waitFor(() =>
      expect(within(listbox).queryByRole('option', { name: /member040/ })).toBeInTheDocument(),
    );
    const scrolled = listbox.scrollTop;
    await expect(scrolled).toBeGreaterThan(0);

    for (const name of ['member040', 'member039']) {
      const option = () => within(listbox).getByRole('option', { name: new RegExp(name) });
      // 保存中は選べぬ。保存が終わるのを待ってから選ぶ
      await waitFor(() => expect(option()).not.toHaveAttribute('data-disabled'));
      await user.click(option());
      await waitFor(() => expect(option()).toHaveAttribute('aria-selected', 'true'));
      // 保存の反映（80ms）と、その後に reka が送るかもしれぬ間を待つ
      await new Promise((resolve) => setTimeout(resolve, 300));
      await expect(option()).toHaveAttribute('aria-selected', 'true');
      // 反映の後も、一覧は先頭の選んだ者へ巻き戻らぬ
      await expect(listbox.scrollTop).toBe(scrolled);
    }
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

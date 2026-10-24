import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ref } from 'vue';

import TaskAssigneePicker from '@/components/tasks/TaskAssigneePicker.vue';
import type { SelectableUser } from '@/components/users/types';
import UserSelector from '@/components/users/UserSelector.vue';
import { accessibleText } from '@/components/__tests__/picker';

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

/** 開いている一覧の項目の名（アバターの頭文字を除く）。 */
const optionNames = (listbox: HTMLElement) =>
  within(listbox).getAllByRole('option').map(accessibleText);

const meta = {
  title: 'Components/UserSelector',
  component: UserSelector,
  args: { label: '担当者', users: people(5), modelValue: [] },
} satisfies Meta<typeof UserSelector>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 自分が先頭、その他は名の順。名の部分一致で絞り込み、複数は選んでも開いたまま。 */
export const Default: Story = {
  render: () => pickerStory(people(12), ['user-2', 'user-1'], 'user-9'),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);
    const user = userEvent.setup();

    const trigger = canvas.getByRole('button', { name: /^担当者: / });
    await user.click(trigger);
    const listbox = await body.findByRole('listbox');
    await expect(optionNames(listbox).slice(0, 3)).toEqual(['frank（自分）', 'alice', 'bob']);

    // 絞り込んでキーボードで選ぶ
    const input = body.getByRole('searchbox', { name: 'メンバーを検索' });
    await waitFor(() => expect(input).toHaveFocus());
    await user.type(input, 'ali');
    await expect(optionNames(listbox)).toEqual(['alice']);
    await user.keyboard('{ArrowDown}{Enter}');
    await waitFor(() =>
      expect(within(listbox).getByRole('option', { name: 'alice' })).toHaveAttribute(
        'aria-selected',
        'true',
      ),
    );
    await expect(trigger).toHaveAttribute('aria-expanded', 'true');

    // Esc で閉じて trigger へ戻る
    await user.keyboard('{Escape}');
    await waitFor(() => expect(trigger).toHaveAttribute('aria-expanded', 'false'));
    await expect(trigger).toHaveFocus();
  },
};

/**
 * 多くの人数。開くまでの時間と絞り込みの応えを測る。Picker は一覧を仮想化しないので、
 * 全員が DOM に載る。測った値は console に `MEASURE` として出す（閾値は緩く置き、遅さの退行だけを捕まえる）。
 */
export const ManyUsers: Story = {
  render: () => pickerStory(people(500), []),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);
    const user = userEvent.setup();

    const t0 = performance.now();
    await user.click(canvas.getByRole('button', { name: '担当者' }));
    const listbox = await body.findByRole('listbox');
    await waitFor(() => expect(within(listbox).getAllByRole('option')).toHaveLength(500));
    const openMs = performance.now() - t0;

    const input = body.getByRole('searchbox', { name: 'メンバーを検索' });
    const t1 = performance.now();
    await user.type(input, 'member500');
    await waitFor(() => expect(optionNames(listbox)).toEqual(['member500']));
    const filterMs = performance.now() - t1;

    console.log(
      `MEASURE users=500 open=${openMs.toFixed(0)}ms filter(9 keys)=${filterMs.toFixed(0)}ms`,
    );
    await expect(openMs).toBeLessThan(3000);
    await user.keyboard('{Escape}');
  },
};

/**
 * 保存が遅れて反映される呼び手（担当者は一人ずつ POST して再取得し、保存中は選択を止める）。
 * 押した項はすぐ選ばれて見え、保存中も一覧は開いたまま、送った位置から動かない。
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

    const trigger = canvas.getByRole('button', { name: /^担当者: / });
    await user.click(trigger);
    const listbox = await body.findByRole('listbox');
    // 一覧か、それを包む窓のどちらかが送られる
    const panel = listbox.closest<HTMLElement>('[role="group"]')!;
    const scroller = [listbox, panel].find((el) => el.scrollHeight > el.clientHeight)!;
    scroller.scrollTop = scroller.scrollHeight;
    const scrolled = scroller.scrollTop;
    await expect(scrolled).toBeGreaterThan(0);

    for (const name of ['member040', 'member039']) {
      const option = () => within(listbox).getByRole('option', { name });
      await user.click(option());
      await expect(option()).toHaveAttribute('aria-selected', 'true');
      // 保存の反映（80ms）を待つ。保存中も一覧は閉じない
      await new Promise((resolve) => setTimeout(resolve, 300));
      await expect(option()).toHaveAttribute('aria-selected', 'true');
      await expect(trigger).toHaveAttribute('aria-expanded', 'true');
      await expect(scroller.scrollTop).toBe(scrolled);
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
      name: '担当者: alice、bob、kousuke、sousuke、yupix',
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

    await user.click(canvas.getByRole('button', { name: '担当者' }));
    const listbox = await body.findByRole('listbox');
    const rect = listbox.closest('[role="group"]')!.getBoundingClientRect();
    await expect(rect.left).toBeGreaterThanOrEqual(0);
    await expect(rect.right).toBeLessThanOrEqual(window.innerWidth);
    for (const option of within(listbox).getAllByRole('option')) {
      // WCAG 2.2 の Target Size（最小）: 24 CSS px
      await expect(option.getBoundingClientRect().height).toBeGreaterThanOrEqual(24);
    }
    await user.keyboard('{Escape}');
  },
};

/** 取得中・失敗・0 人・選択ありの見え方。押すと窓の中身が開く。 */
export const States: Story = {
  render: () => ({
    components: { UserSelector },
    setup: () => ({ users: people(3), retry: () => {} }),
    template: `<div class="flex flex-wrap gap-3">
      <UserSelector label="取得中" :users="[]" :model-value="[]" :state="{ loading: true }" />
      <UserSelector label="失敗" :users="[]" :model-value="[]" :state="{ error: true, onRetry: retry }" />
      <UserSelector label="0 人" :users="[]" :model-value="[]" empty-text="選べる利用者がいません" />
      <UserSelector label="選択あり" :users="users" :model-value="['user-2']" current-user-id="user-3" />
    </div>`,
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    // 閉じた窓の中身も DOM にはあるので、字で確かめる
    await expect(canvas.getByText('読み込み中…')).toBeInTheDocument();
    await expect(canvas.getByText('候補を読み込めませんでした')).toBeInTheDocument();
    await expect(canvas.getByText('選べる利用者がいません')).toBeInTheDocument();

    await user.click(canvas.getByRole('button', { name: /^選択あり/ }));
    const listbox = await canvas.findByRole('listbox');
    await expect(within(listbox).getByRole('option', { selected: true })).toHaveAccessibleName(
      'sousuke',
    );
    await expect(optionNames(listbox)[0]).toBe('kousuke（自分）');
    await user.keyboard('{Escape}');
  },
};

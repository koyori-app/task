import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import EmojiPicker from '@/components/reactions/EmojiPicker.vue';
import ReactionBar from '@/components/reactions/ReactionBar.vue';
import ReactionChip from '@/components/reactions/ReactionChip.vue';
import ReactionPeopleCard from '@/components/reactions/ReactionPeopleCard.vue';
import ReactionPeoplePanel from '@/components/reactions/ReactionPeoplePanel.vue';
import type { ReactionSummary, ReactionUser } from '@/lib/reactions';

/**
 * タスク詳細のコメントと概要の絵文字リアクション。ピッカーと名前は emojibase の実物を読む。
 * a11y の検査は todo にせず落とす。
 */
const meta = {
  title: 'Tasks/Reactions',
  parameters: { a11y: { test: 'error' } },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

const ME = 'user-0';

function people(count: number): ReactionUser[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `user-${index}`,
    name: index === 0 ? 'yupix' : `メンバー ${index}`,
    avatar_url: null,
  }));
}

const fetchUsers = async (emoji: string) => people(emoji === '👍' ? 12 : 3);

/**
 * ホバーとフォーカスは CSS の疑似クラスで play から再現できないので、札の変数を同じトークンで上書きする。
 */
const HOVER_STYLE =
  '--chip-bg: var(--koyori-color-surface-active); --chip-border: var(--koyori-color-border-strong);';
const REACTED_HOVER_STYLE =
  '--chip-bg: var(--koyori-color-accent-hover); --chip-border: var(--koyori-color-selection-indicator);';
const FOCUS_STYLE =
  'outline: var(--koyori-focus-width) solid var(--koyori-focus-color); outline-offset: var(--koyori-focus-offset);';

/** 札の 5 状態（通常・ホバー・自分が押した・自分＋ホバー・フォーカス） */
export const ChipStates: Story = {
  render: () => ({
    components: { ReactionChip },
    setup: () => ({ HOVER_STYLE, REACTED_HOVER_STYLE, FOCUS_STYLE }),
    template: `
      <div class="flex flex-wrap items-center gap-3">
        <ReactionChip emoji="👍" :count="3" :reacted="false" />
        <ReactionChip emoji="👍" :count="3" :reacted="false" :style="HOVER_STYLE" />
        <ReactionChip emoji="👍" :count="3" :reacted="true" />
        <ReactionChip emoji="👍" :count="3" :reacted="true" :style="REACTED_HOVER_STYLE" />
        <ReactionChip emoji="👍" :count="3" :reacted="false" :style="FOCUS_STYLE" />
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    const pressed = within(canvasElement).getAllByRole('button', { pressed: true });
    await expect(pressed).toHaveLength(2);
  },
};

/** 件数（1 / 12 / 128 / 999+） */
export const ChipCounts: Story = {
  render: () => ({
    components: { ReactionChip },
    template: `
      <div class="flex flex-wrap items-center gap-3">
        <ReactionChip emoji="🎉" :count="1" :reacted="false" />
        <ReactionChip emoji="🎉" :count="12" :reacted="false" />
        <ReactionChip emoji="🎉" :count="128" :reacted="false" />
        <ReactionChip emoji="🎉" :count="1000" :reacted="false" />
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain('999+');
  },
};

/** 長い名前のカード（1 行で省略し title に全文） */
export const CardLongName: Story = {
  render: () => ({
    components: { ReactionPeopleCard },
    setup: () => ({
      users: [
        {
          id: ME,
          name: 'とても長い名前の利用者がここにいて一行に収まらないので省略されるはずの人',
          avatar_url: null,
        },
        { id: 'user-1', name: 'taro', avatar_url: null },
      ],
      ME,
    }),
    template: `<ReactionPeopleCard emoji="👍" :count="2" :users="users" :current-user-id="ME" />`,
  }),
};

/** 6 人以上のカード（5 人で切れて「すべて見る」が出る） */
export const CardManyPeople: Story = {
  render: () => ({
    components: { ReactionPeopleCard },
    setup: () => ({ users: people(5), ME }),
    template: `<ReactionPeopleCard emoji="👍" :count="12" :users="users" :current-user-id="ME" />`,
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('button', { name: 'すべて見る（12 人）' })).toBeVisible();
    await expect(canvas.getAllByRole('listitem')).toHaveLength(5);
  },
};

const MANY_EMOJI = ['👍', '❤️', '🎉', '😄', '👀', '🙏', '✅', '🚀', '😂', '🔥', '💯'];

function manyReactions(): ReactionSummary[] {
  return MANY_EMOJI.map((emoji, index) => ({
    emoji,
    count: index * 7 + 1,
    reacted: index % 3 === 0,
    users: people(Math.min(index * 7 + 1, 5)),
  }));
}

/** 種類が多い行（11 種が幅 240 で折り返す）。カードと全員の一覧、ピッカーもここから開ける */
export const ManyReactionsWrap: Story = {
  render: () => ({
    components: { ReactionBar },
    setup: () => ({
      reactions: manyReactions(),
      fetchUsers,
      ME,
      toggle: fn(),
      add: fn(),
      profileHref: (id: string) => `#${id}`,
    }),
    template: `
      <div class="w-60">
        <ReactionBar
          :reactions="reactions"
          can-react
          :fetch-users="fetchUsers"
          :current-user-id="ME"
          :profile-href="profileHref"
          @toggle="toggle"
          @add="add"
        />
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    const chips = canvasElement.querySelectorAll<HTMLElement>('[data-reaction-chip]');
    await expect(chips).toHaveLength(MANY_EMOJI.length);
    const tops = new Set(Array.from(chips).map((chip) => chip.getBoundingClientRect().top));
    await expect(tops.size).toBeGreaterThan(1);
    await expect(canvasElement.scrollWidth).toBeLessThanOrEqual(canvasElement.clientWidth);
  },
};

/** 失敗の文言は行の下 */
export const WithError: Story = {
  render: () => ({
    components: { ReactionBar },
    setup: () => ({ reactions: manyReactions().slice(0, 3), fetchUsers }),
    template: `
      <ReactionBar
        :reactions="reactions"
        can-react
        always-show-add
        error="リアクションを送れませんでした"
        :fetch-users="fetchUsers"
      />`,
  }),
};

/** 開いたボタンの近くに top layer で出す。入れ物の中でも切れない */
export const AddButtonOpensPicker: Story = {
  render: () => ({
    components: { ReactionBar },
    setup: () => ({ fetchUsers }),
    template: `
      <div class="h-16 w-60 overflow-hidden rounded-md border border-(--koyori-color-border) p-2">
        <ReactionBar :reactions="[]" can-react always-show-add :fetch-users="fetchUsers" />
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const add = canvas.getByRole('button', { name: 'リアクションを追加' });
    await userEvent.click(add);
    const search = await canvas.findByRole('searchbox', { name: '絵文字を検索' });
    await waitFor(() => expect(search).toHaveFocus());
    await canvas.findByRole('grid', { name: '絵文字' }, { timeout: 10000 });
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(canvas.queryByRole('dialog')).toBeNull());
    await expect(add).toHaveFocus();
  },
};

const pickerStory = {
  components: { EmojiPicker },
  setup: () => ({ selected: ['👍', '🎉'] }),
  template: `<EmojiPicker :selected="selected" />`,
};

/** ピッカーの既定（emojibase の import を実物で動かす） */
export const PickerDefault: Story = {
  render: () => pickerStory,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const grid = await canvas.findByRole('grid', { name: '絵文字' }, { timeout: 10000 });
    await expect(
      within(grid).getAllByRole('gridcell', { name: 'サムズアップ' })[0],
    ).toHaveAttribute('aria-selected', 'true');

    await userEvent.click(canvas.getByRole('button', { name: '旗' }));
    await waitFor(() =>
      expect(canvas.getByRole('button', { name: '旗' })).toHaveAttribute('aria-current', 'true'),
    );
    await waitFor(() => expect(grid.scrollTop).toBeGreaterThan(0));
    grid.scrollTop = 0;
    await waitFor(() =>
      expect(canvas.getByRole('button', { name: 'よく使う' })).toHaveAttribute(
        'aria-current',
        'true',
      ),
    );
  },
};

/** ピッカーの検索中 */
export const PickerSearching: Story = {
  render: () => pickerStory,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('grid', { name: '絵文字' }, { timeout: 10000 });
    await userEvent.type(canvas.getByRole('searchbox'), 'ハート');
    await canvas.findByRole('columnheader', { name: /検索結果 \d+ 件/ });
    await expect(canvas.queryByRole('group', { name: 'カテゴリ' })).toBeNull();
  },
};

/** ピッカーの結果なし */
export const PickerNoResults: Story = {
  render: () => pickerStory,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('grid', { name: '絵文字' }, { timeout: 10000 });
    await userEvent.type(canvas.getByRole('searchbox'), 'そんなえもじはない');
    await canvas.findByText('絵文字が見つかりません');
  },
};

/** 全員の一覧 */
export const PeoplePanel: Story = {
  render: () => ({
    components: { ReactionPeoplePanel },
    setup: () => ({
      reactions: manyReactions(),
      fetchUsers,
      ME,
      profileHref: (id: string) => `#${id}`,
    }),
    template: `
      <ReactionPeoplePanel
        :reactions="reactions"
        initial-emoji="👍"
        :fetch-users="fetchUsers"
        :current-user-id="ME"
        :profile-href="profileHref"
      />`,
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('link', { name: 'yupix' });
    await expect(canvas.getAllByRole('listitem')).toHaveLength(12);
  },
};

/** 全員の一覧の読み込み中（Skeleton） */
export const PeoplePanelLoading: Story = {
  render: () => ({
    components: { ReactionPeoplePanel },
    setup: () => ({
      reactions: manyReactions(),
      fetchUsers: () => new Promise<ReactionUser[]>(() => undefined),
    }),
    template: `<ReactionPeoplePanel :reactions="reactions" initial-emoji="👍" :fetch-users="fetchUsers" />`,
  }),
};

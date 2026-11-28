import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { provide } from 'vue';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import SidebarUserPanel from '@/components/sidebar/SidebarUserPanel.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const pageContext = (urlPathname: string) => () => ({
  setup() {
    provide(PAGE_CONTEXT_KEY, { urlPathname, routeParams: {} });
  },
  template: '<story />',
});

const onLogout = fn();

const meta = {
  title: 'Sidebar/SidebarUserPanel',
  component: SidebarUserPanel,
  args: {
    user: { name: 'yupix', email: 'm@example.com', avatar: '' },
    onLogout,
  },
  decorators: [
    pageContext('/acme'),
    // サイドバーの下端と同じ幅。カードはパネルの上に開く
    () => ({
      template: '<div class="flex h-[480px] w-60 flex-col justify-end border p-2"><story /></div>',
    }),
  ],
} satisfies Meta<typeof SidebarUserPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 名前の下にメールアドレスを出す（閉じたカードの中の同じ文字は読み上げに出ない）
    await expect(canvas.getByRole('button', { name: /yupix/ })).toHaveTextContent('m@example.com');
    await expect(canvas.getByRole('link', { name: 'アカウント設定' })).not.toHaveAttribute(
      'aria-current',
    );
  },
};

/** ユーザーを押してプロフィールカードを開いた状態 */
export const CardOpen: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: /yupix/ }));
    const card = await within(canvasElement.ownerDocument.body).findByRole('dialog', {
      name: 'アカウント',
    });
    await waitFor(() => expect(card).toBeVisible());
    const card$ = within(card);
    await expect(card$.getByRole('link', { name: 'プロフィールを編集' })).toBeVisible();
    await expect(card$.queryByText('通知')).toBeNull();
    await userEvent.click(card$.getByRole('button', { name: 'ログアウト' }));
    await expect(onLogout).toHaveBeenCalledTimes(1);
  },
};

/** アカウント設定（/settings 配下）にいるあいだは歯車に印を付ける */
export const InAccountSettings: Story = {
  decorators: [pageContext('/settings/security')],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('link', { name: 'アカウント設定' })).toHaveAttribute(
      'aria-current',
      'page',
    );
  },
};

/** サイドバーを畳んだ状態。アバターだけを出し、歯車は出さない */
export const Compact: Story = {
  args: { rail: true },
  decorators: [
    () => ({
      template: '<div class="flex h-[480px] w-14 flex-col justify-end"><story /></div>',
    }),
  ],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.queryByRole('link', { name: 'アカウント設定' })).toBeNull();
    await userEvent.click(canvas.getByRole('button', { name: 'yupix' }));
    await waitFor(() =>
      expect(
        within(canvasElement.ownerDocument.body).getByRole('dialog', { name: 'アカウント' }),
      ).toBeVisible(),
    );
  },
};

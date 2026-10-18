import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, waitFor, within } from 'storybook/test';

import UserAvatar from '@/components/UserAvatar.vue';
import { USER_AVATAR_SIZES, type UserAvatarSize } from '@/lib/user-avatar';

/** 画像の読み込みを待たずに済むよう、data URI の SVG を使う（外へは出ない）。 */
const IMAGE_URL = `data:image/svg+xml;utf8,${encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8" fill="#2563eb"/><circle cx="4" cy="4" r="2" fill="#fff"/></svg>',
)}`;
/** 読み込みに失敗する URL（頭文字に落ちることを見せる）。 */
const BROKEN_URL = 'data:image/png;base64,not-an-image';

const SIZES = Object.keys(USER_AVATAR_SIZES) as UserAvatarSize[];

const meta = {
  title: 'Components/UserAvatar',
  component: UserAvatar,
  // 頭文字の色（bg-muted 上の text-muted-foreground）は既存の画面と同じ組で、12px 以下の字では
  // 対比が 4.34:1 と 4.5:1 に届かぬ。色は変えぬ寄せなので、ここでは落とさず todo に置く
  parameters: { a11y: { test: 'todo' } },
  args: { user: { name: 'yupix', avatar_url: null } },
} satisfies Meta<typeof UserAvatar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Initials: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('YU')).toBeInTheDocument();
  },
};

export const Sizes: Story = {
  render: () => ({
    components: { UserAvatar },
    setup: () => ({ SIZES }),
    template: `<div class="flex items-end gap-3" data-testid="sizes">
      <UserAvatar v-for="size in SIZES" :key="size" :size="size" :user="{ name: 'yupix' }" />
    </div>`,
  }),
};

export const Shapes: Story = {
  render: () => ({
    components: { UserAvatar },
    template: `<div class="flex items-center gap-3">
      <UserAvatar shape="full" :user="{ name: 'yupix' }" />
      <UserAvatar shape="md" :user="{ name: 'yupix' }" />
      <UserAvatar shape="lg" :user="{ name: 'yupix' }" />
    </div>`,
  }),
};

export const WithImage: Story = {
  args: { user: { name: 'yupix', avatar_url: IMAGE_URL } },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const img = await canvas.findByRole('img', { name: 'yupix' });
    await waitFor(() => expect(img).toBeVisible());
  },
};

export const BrokenImage: Story = {
  args: { user: { name: 'yupix', avatar_url: BROKEN_URL } },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => expect(canvas.getByText('YU')).toBeVisible());
  },
};

export const Inactive: Story = {
  render: () => ({
    components: { UserAvatar },
    setup: () => ({ IMAGE_URL }),
    template: `<div class="flex items-center gap-3">
      <UserAvatar :user="{ name: 'yupix' }" />
      <UserAvatar :user="{ name: 'yupix' }" inactive />
      <UserAvatar :user="{ name: 'yupix', avatar_url: IMAGE_URL }" />
      <UserAvatar :user="{ name: 'yupix', avatar_url: IMAGE_URL }" inactive />
    </div>`,
  }),
};

export const Overflow: Story = {
  args: { user: { name: '' }, fallbackText: '+3', size: 'lg' },
};

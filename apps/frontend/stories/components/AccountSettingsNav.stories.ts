import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, within } from 'storybook/test';
import { createPinia, setActivePinia } from 'pinia';

import AccountSettingsNav from '@/components/settings/AccountSettingsNav.vue';
import { usePreferencesStore } from '@/stores/preferences';

/** アカウント設定の一覧。「実験的機能」は環境設定で表示を有効にした時だけ、一番下に出る。 */
const meta = {
  title: 'Components/AccountSettingsNav',
  component: AccountSettingsNav,
  // 色の対比などの a11y 違反を、この画面では落ちる検査（error）にする
  parameters: { a11y: { test: 'error' } },
  args: { currentPath: '/settings/preferences' },
  render: (args) => ({
    components: { AccountSettingsNav },
    setup: () => ({ args }),
    template: '<div class="w-56 p-6"><AccountSettingsNav v-bind="args" /></div>',
  }),
  beforeEach: () => {
    setActivePinia(createPinia());
  },
} satisfies Meta<typeof AccountSettingsNav>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ExperimentsHidden: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('link', { name: '環境設定' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    await expect(canvas.queryByRole('link', { name: '実験的機能' })).toBeNull();
  },
};

export const ExperimentsShown: Story = {
  args: { currentPath: '/settings/experiments' },
  beforeEach: () => {
    setActivePinia(createPinia());
    usePreferencesStore().set('showExperiments', true);
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const link = await canvas.findByRole('link', { name: '実験的機能' });
    await expect(link).toHaveAttribute('href', '/settings/experiments');
    await expect(link).toHaveAttribute('aria-current', 'page');
  },
};

import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { createPinia, setActivePinia } from 'pinia';

import PreferencesSection from '@/components/settings/PreferencesSection.vue';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';

/**
 * 環境設定。この端末のこのブラウザだけで効く設定で、アカウントには保存されぬ。
 * 残った値は mount の後に出る（SSR の HTML と最初の描画を揃えるため）。
 */
const meta = {
  title: 'Components/PreferencesSection',
  component: PreferencesSection,
  // 色の対比などの a11y 違反を、この画面では落ちる検査（error）にする
  parameters: { a11y: { test: 'error' } },
  render: () => ({
    components: { PreferencesSection },
    template: '<div class="max-w-2xl p-6"><PreferencesSection /></div>',
  }),
  beforeEach: () => {
    setActivePinia(createPinia());
  },
} satisfies Meta<typeof PreferencesSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText(/この端末のこのブラウザだけで効く設定です/)).toBeInTheDocument();
    const toggle = await canvas.findByRole('switch', { name: '実験的機能を表示する' });
    await waitFor(() => expect(toggle).toBeEnabled());
    await expect(toggle).toHaveAttribute('aria-checked', 'false');
    await expect(canvas.queryByRole('status')).toBeNull();
  },
};

export const ExperimentsShown: Story = {
  beforeEach: () => {
    setActivePinia(createPinia());
    usePreferencesStore().set('showExperiments', true);
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByRole('switch', { name: '実験的機能を表示する' });
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
  },
};

/** 表示を無効にしたまま有効のフラグが残る時は、働き続けていることを画面で知らせる。 */
export const HiddenFlagStillEnabled: Story = {
  beforeEach: () => {
    setActivePinia(createPinia());
    useExperimentsStore().set('koyoriArcIntegration', true);
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole('status')).toHaveTextContent('1 件');
  },
};

export const Toggle: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByRole('switch', { name: '実験的機能を表示する' });
    await waitFor(() => expect(toggle).toBeEnabled());
    await userEvent.click(toggle);
    await expect(toggle).toHaveAttribute('aria-checked', 'true');
    await userEvent.click(canvas.getByRole('button', { name: 'すべて既定に戻す' }));
    await expect(toggle).toHaveAttribute('aria-checked', 'false');
  },
};

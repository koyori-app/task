import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { createPinia, setActivePinia } from 'pinia';

import ExperimentsSection from '@/components/settings/ExperimentsSection.vue';
import { useExperimentsStore } from '@/stores/experiments';
import { usePreferencesStore } from '@/stores/preferences';

/**
 * 実験的機能。環境設定で表示を有効にした時だけ、フラグの一覧を出す。
 * 表示が無効のまま直に開いた時は、送り返さず案内を出す。
 */
const meta = {
  title: 'Components/ExperimentsSection',
  component: ExperimentsSection,
  // 色の対比などの a11y 違反を、この画面では落ちる検査（error）にする
  parameters: { a11y: { test: 'error' } },
  render: () => ({
    components: { ExperimentsSection },
    template: '<div class="max-w-2xl p-6"><ExperimentsSection /></div>',
  }),
  beforeEach: () => {
    setActivePinia(createPinia());
    usePreferencesStore().set('showExperiments', true);
  },
} satisfies Meta<typeof ExperimentsSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const FlagList: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText('この端末のこのブラウザだけで有効です。不安定な機能を含みます。'),
    ).toBeInTheDocument();
    const toggle = canvas.getByRole('switch', { name: 'koyori-arc の統合' });
    await expect(toggle).toHaveAttribute('aria-checked', 'false');
  },
};

export const FlagEnabled: Story = {
  beforeEach: () => {
    setActivePinia(createPinia());
    usePreferencesStore().set('showExperiments', true);
    useExperimentsStore().set('koyoriArcIntegration', true);
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByRole('switch', { name: 'koyori-arc の統合' });
    await expect(toggle).toHaveAttribute('aria-checked', 'true');
    await userEvent.click(canvas.getByRole('button', { name: 'すべて既定に戻す' }));
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));
  },
};

/** 表示が無効のまま /settings/experiments を直に開いた時の案内。 */
export const DisplayDisabled: Story = {
  beforeEach: () => {
    setActivePinia(createPinia());
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('実験的機能の表示が無効です。')).toBeInTheDocument();
    await expect(canvas.getByRole('link', { name: '環境設定を開く' })).toHaveAttribute(
      'href',
      '/settings/preferences',
    );
    await expect(canvas.queryByRole('switch')).toBeNull();
  },
};

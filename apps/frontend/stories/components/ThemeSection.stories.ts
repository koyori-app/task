import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';

import ThemeSection from '@/components/settings/ThemeSection.vue';
import { htmlTheme } from '../html-theme';

/**
 * テーマ。画面の配色（ライト／ダーク）を ThemeSwitch で切り替える。
 * この端末のこのブラウザだけで効く設定で、アカウントには保存されぬ（localStorage の koyori-theme）。
 * スイッチの状態は mount の後に html の data-theme から決まる。
 */
const meta = {
  title: 'Components/ThemeSection',
  component: ThemeSection,
  // 色の対比などの a11y 違反を、この画面では落ちる検査（error）にする
  parameters: { a11y: { test: 'error' } },
  render: () => ({
    components: { ThemeSection },
    template: '<div class="max-w-2xl p-6"><ThemeSection /></div>',
  }),
  beforeEach: () => {
    localStorage.removeItem('koyori-theme');
    return () => localStorage.removeItem('koyori-theme');
  },
} satisfies Meta<typeof ThemeSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Dark: Story = {
  name: 'ダーク',
  beforeEach: htmlTheme('dark'),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText(/この端末のこのブラウザだけで効く設定です/)).toBeInTheDocument();
    await expect(
      canvas.getByText('オンで暗い配色、オフで明るい配色になります。'),
    ).toBeInTheDocument();
    const toggle = canvas.getByRole('switch', { name: 'ダークテーマ' });
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
  },
};

export const Light: Story = {
  name: 'ライト',
  beforeEach: htmlTheme('light'),
  play: async ({ canvasElement }) => {
    const toggle = within(canvasElement).getByRole('switch', { name: 'ダークテーマ' });
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));
  },
};

export const Toggle: Story = {
  name: '切り替え（ライトからダークへ）',
  beforeEach: htmlTheme('light'),
  play: async ({ canvasElement }) => {
    const toggle = within(canvasElement).getByRole('switch', { name: 'ダークテーマ' });
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));
    await userEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
    await expect(document.documentElement.dataset.theme).toBe('dark');
    await expect(localStorage.getItem('koyori-theme')).toBe('dark');
  },
};

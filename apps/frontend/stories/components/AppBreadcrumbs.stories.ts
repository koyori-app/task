import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, within } from 'storybook/test';
import AppBreadcrumbs from '@/components/AppBreadcrumbs.vue';
import { breadcrumbSegments } from '@/lib/breadcrumbs';

const meta = {
  title: 'Navigation/AppBreadcrumbs',
  component: AppBreadcrumbs,
  parameters: { a11y: { test: 'error' } },
  args: {
    loading: false,
    segments: breadcrumbSegments([
      { name: 'ホーム', href: '/acme/my-tasks' },
      { name: 'エンジニアリング', href: '/acme/projects/ENG/tasks' },
      { name: 'リリース準備', href: '/acme/projects/ENG/tasks/ENG-2' },
      { name: '動作を確認する', href: '/acme/projects/ENG/tasks/ENG-3' },
    ]),
  },
} satisfies Meta<typeof AppBreadcrumbs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const WithParent: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('navigation', { name: 'パンくず' })).toBeVisible();
    await expect(canvas.getByRole('link', { name: 'ホーム' })).toHaveAttribute(
      'href',
      '/acme/my-tasks',
    );
    await expect(canvas.getByText('動作を確認する')).toHaveAttribute('aria-current', 'page');
    await expect(canvas.getAllByRole('link')).toHaveLength(3);
  },
};

export const Loading: Story = {
  args: { loading: true, segments: [] },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByRole('status')).toHaveAccessibleName(
      'パンくずを読み込み中',
    );
  },
};

export const NarrowLongNames: Story = {
  decorators: [() => ({ template: '<div style="width: 280px"><story /></div>' })],
  args: {
    segments: breadcrumbSegments([
      { name: 'ホーム', href: '/acme/my-tasks' },
      { name: '非常に長いプロジェクト名を表示する場合の例', href: '/acme/projects/ENG/tasks' },
      {
        name: '非常に長いタスク名でもレイアウトが崩れず全文を確認できる',
        href: '/acme/projects/ENG/tasks/ENG-3',
      },
    ]),
  },
  play: async ({ canvasElement }) => {
    const nav = within(canvasElement).getByRole('navigation');
    await expect(nav.getBoundingClientRect().width).toBeLessThanOrEqual(280);
    const list = within(canvasElement).getByRole('list');
    await expect(getComputedStyle(list).overflowX).toBe('auto');
    await expect(list.scrollWidth).toBeGreaterThan(list.clientWidth);
    const current = canvasElement.querySelector('[aria-current="page"]')!;
    await expect(current.getAttribute('title')).toBe(current.textContent);
  },
};

export const Omitted: Story = { args: { segments: [] } };

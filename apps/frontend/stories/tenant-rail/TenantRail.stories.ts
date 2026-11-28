import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';

import TenantRail from '@/components/tenant-rail/TenantRail.vue';
import type { Tenant } from '@/stores/tenant';

const tenant = (n: number, name: string, icon: Partial<Tenant> = {}): Tenant => ({
  id: `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`,
  name,
  display_id: name.toLowerCase().replace(/\s+/g, '-'),
  description: '',
  icon_url: '',
  icon_emoji: null,
  owner_id: '00000000-0000-4000-8000-000000000001',
  require_2fa: false,
  membership: 'Owner',
  ...icon,
});

// 絵文字・頭文字・画像（読み込めないときは頭文字）を並べる
const tenants = [
  tenant(10, 'Acme Inc', { icon_emoji: '🚀' }),
  tenant(20, 'Globex'),
  tenant(30, 'Initech', { icon_url: 'data:image/svg+xml,%3Csvg/%3E' }),
];

const onSelect = fn();
const onRetry = fn();
const onCreate = fn();

const meta = {
  title: 'TenantRail/TenantRail',
  component: TenantRail,
  args: {
    tenants,
    selectedTenantId: tenants[0]!.id,
    onSelect,
    onRetry,
    onCreate,
    onMenu: fn(),
  },
  // レールは画面の上端から下端まで伸びる。高さを決める入れ物で包む
  decorators: [() => ({ template: '<div class="flex h-[480px] w-16 border-r"><story /></div>' })],
} satisfies Meta<typeof TenantRail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const rail = canvas.getByRole('navigation', { name: 'テナント' });
    await expect(within(rail).getByRole('button', { name: 'Acme Inc' })).toHaveAttribute(
      'aria-current',
      'true',
    );
    await userEvent.click(within(rail).getByRole('button', { name: 'Globex' }));
    await expect(onSelect).toHaveBeenCalledWith(tenants[1]);
    await userEvent.click(within(rail).getByRole('button', { name: 'テナントを作成' }));
    await expect(onCreate).toHaveBeenCalledTimes(1);
  },
};

export const Loading: Story = {
  args: { tenants: [], selectedTenantId: null, loading: true },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('テナントを読み込み中…')).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'テナントを作成' })).toBeDisabled();
  },
};

export const Failed: Story = {
  args: { tenants: [], selectedTenantId: null, error: 'テナント一覧を取得できませんでした' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(
      canvas.getByRole('button', { name: 'テナント一覧を取得できませんでした（再試行）' }),
    );
    await expect(onRetry).toHaveBeenCalledTimes(1);
  },
};

export const Empty: Story = {
  args: { tenants: [], selectedTenantId: null },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('所属テナントがありません')).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'テナントを作成' })).toBeEnabled();
  },
};

/** 30 件。縦に溢れても、最後の項目と「＋」までスクロールで届く */
export const Many: Story = {
  args: {
    tenants: Array.from({ length: 30 }, (_, i) => tenant(100 + i, `Tenant ${i + 1}`)),
    selectedTenantId: null,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const nav = canvas.getByRole('navigation', { name: 'テナント' });
    // レールは入れ物の高さに収まり、溢れた分はレールの中でスクロールする
    await expect(nav.getBoundingClientRect().height).toBe(480);
    await expect(nav.scrollHeight).toBeGreaterThan(nav.clientHeight);
    const create = canvas.getByRole('button', { name: 'テナントを作成' });
    create.scrollIntoView({ block: 'end' });
    const frame = nav.getBoundingClientRect();
    const box = create.getBoundingClientRect();
    await expect(box.bottom).toBeLessThanOrEqual(frame.bottom + 1);
    await expect(box.top).toBeGreaterThanOrEqual(frame.top - 1);
  },
};

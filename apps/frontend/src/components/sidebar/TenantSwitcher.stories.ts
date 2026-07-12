import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { createPinia, setActivePinia } from 'pinia';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { Tenant } from '@/stores/tenant';
import { useTenantStore } from '@/stores/tenant';
import { SidebarProvider } from '@/components/ui/sidebar';
import TenantSwitcher from './TenantSwitcher.vue';

const ownerId = '00000000-0000-4000-8000-000000000001';

const tenant = (id: string, name: string, displayId: string): Tenant => ({
  id,
  name,
  display_id: displayId,
  description: `${name} tenant`,
  icon_url: '',
  owner_id: ownerId,
  require_2fa: false,
});

const primaryTenant = tenant('00000000-0000-4000-8000-000000000010', 'Acme', 'acme');
const secondaryTenant = tenant('00000000-0000-4000-8000-000000000020', 'Globex', 'globex');

const meta = {
  title: 'Sidebar/TenantSwitcher',
  component: TenantSwitcher,
  tags: ['autodocs'],
  render: (args) => ({
    components: { SidebarProvider, TenantSwitcher },
    setup() {
      const pinia = createPinia();
      setActivePinia(pinia);
      const store = useTenantStore();
      store.$patch({
        tenants: args.tenants,
        selectedTenantId: args.selectedTenantId,
        isLoading: args.loading ?? false,
        error: args.error ?? null,
      });
      return { args, store };
    },
    template: `
      <SidebarProvider class="min-h-0 w-80 p-4">
        <TenantSwitcher
          :tenants="store.tenants"
          :selected-tenant-id="store.selectedTenantId"
          :loading="store.isLoading"
          :error="store.error"
          @select="args.onSelect"
          @retry="args.onRetry"
        />
      </SidebarProvider>
    `,
  }),
  args: {
    tenants: [primaryTenant, secondaryTenant],
    selectedTenantId: primaryTenant.id,
    loading: false,
    error: null,
    onSelect: fn(),
    onRetry: fn(),
  },
} satisfies Meta<typeof TenantSwitcher>;

export default meta;
type Story = StoryObj<typeof meta>;

export const MultipleTenants: Story = {
  play: async ({ canvasElement, args }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(page.getByRole('button', { name: /Acme/ }));
    await userEvent.click(await page.findByText('Globex'));
    await expect(args.onSelect).toHaveBeenCalledWith(secondaryTenant);
  },
};

export const Loading: Story = {
  args: {
    tenants: [],
    selectedTenantId: null,
    loading: true,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('テナントを読み込み中…')).toBeInTheDocument();
    await expect(canvas.getByRole('button')).toBeDisabled();
  },
};

export const NoMemberships: Story = {
  args: {
    tenants: [],
    selectedTenantId: null,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('所属テナントなし')).toBeInTheDocument();
    await expect(canvas.getByRole('button')).toBeDisabled();
  },
};

export const ApiError: Story = {
  args: {
    tenants: [],
    selectedTenantId: null,
    error: 'テナント一覧を取得できませんでした',
  },
  play: async ({ canvasElement, args }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(page.getByRole('button', { name: /所属テナントなし/ }));
    await userEvent.click(await page.findByText(/再試行/));
    await expect(args.onRetry).toHaveBeenCalledOnce();
  },
};

export const SingleTenant: Story = {
  args: {
    tenants: [primaryTenant],
    selectedTenantId: primaryTenant.id,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('Acme')).toBeInTheDocument();
    await expect(canvas.getByText('acme')).toBeInTheDocument();
  },
};

export const MissingSelectionFallsBack: Story = {
  args: {
    tenants: [primaryTenant, secondaryTenant],
    selectedTenantId: '00000000-0000-4000-8000-000000000099',
  },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByText('Acme')).toBeInTheDocument();
  },
};

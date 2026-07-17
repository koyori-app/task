import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { components } from '@/generated/api';
import NavProjects from '@/components/sidebar/NavProjects.vue';

type ProjectNavItem = components['schemas']['ProjectResponse'];
import { SidebarProvider } from '@/components/ui/sidebar';

const tenantSlug = 'acme';
const retry = fn();

const projects: ProjectNavItem[] = [
  {
    id: '00000000-0000-4000-8000-000000000010',
    tenant_id: '00000000-0000-4000-8000-000000000001',
    name: 'Design Engineering',
    description: 'Shared',
    key: 'design',
    is_personal: false,
    icon_emoji: null,
    icon_url: null,
    personal_owner_id: null,
  },
  {
    id: '00000000-0000-4000-8000-000000000020',
    tenant_id: '00000000-0000-4000-8000-000000000001',
    name: 'My Tasks',
    description: 'Personal',
    key: 'my',
    is_personal: true,
    icon_emoji: '✅',
    icon_url: null,
    personal_owner_id: '00000000-0000-4000-8000-000000000099',
  },
];

type StoryState = {
  projects: ProjectNavItem[];
  loading?: boolean;
  error?: boolean;
};

const renderWithState = (state: StoryState) => () => ({
  components: { SidebarProvider, NavProjects },
  setup() {
    function onRetry() {
      retry();
    }

    return { tenantSlug, onRetry, state };
  },
  template: `
    <SidebarProvider class="min-h-0 w-80 p-4">
      <NavProjects
        :tenant-slug="tenantSlug"
        :projects="state.projects"
        :loading="state.loading ?? false"
        :error="state.error ?? false"
        @retry="onRetry"
      />
    </SidebarProvider>
  `,
});

const meta = {
  title: 'Sidebar/NavProjects',
  component: NavProjects,
  tags: ['autodocs'],
} satisfies Meta<typeof NavProjects>;

export default meta;
type Story = StoryObj;

export const ProjectList: Story = {
  render: renderWithState({ projects }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('Design Engineering')).toBeInTheDocument();
    // 行クリックで展開 → タスク/ラベル/設定の子リンク
    await userEvent.click(canvas.getByRole('button', { name: /Design Engineering/ }));
    await expect(canvas.getByRole('link', { name: 'タスク' })).toHaveAttribute(
      'href',
      '/acme/projects/design/tasks',
    );
    await expect(canvas.getByRole('link', { name: 'ラベル' })).toHaveAttribute(
      'href',
      '/acme/projects/design/labels',
    );
    await expect(canvas.getByRole('link', { name: '設定' })).toHaveAttribute(
      'href',
      '/acme/projects/design/settings',
    );
  },
};

export const Loading: Story = {
  render: renderWithState({ projects: [], loading: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('プロジェクトを読み込み中…')).toBeInTheDocument();
  },
};

export const Empty: Story = {
  render: renderWithState({ projects: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('プロジェクトはまだありません。')).toBeInTheDocument();
    // グループの「+」と空状態カードのボタンの2つが並存する
    const createButtons = canvas.getAllByRole('button', { name: /プロジェクトを作成/ });
    await expect(createButtons.length).toBe(2);
  },
};

export const ApiError: Story = {
  render: renderWithState({ projects: [], error: true }),
  play: async ({ canvasElement }) => {
    retry.mockClear();
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: /再試行/ }));
    await expect(retry).toHaveBeenCalledOnce();
  },
};

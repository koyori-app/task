import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { Sidebar } from '@koyori-app/ui-vue';
import type { components } from '@/generated/api';
import NavProjects from '@/components/sidebar/NavProjects.vue';

type ProjectNavItem = components['schemas']['ProjectResponse'];

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
  /** アイコンだけの表示（広い画面で畳んだサイドバー） */
  rail?: boolean;
};

const renderWithState = (state: StoryState) => () => ({
  components: { Sidebar, NavProjects },
  setup() {
    function onRetry() {
      retry();
    }

    return { tenantSlug, onRetry, state };
  },
  template: `
    <div class="flex h-[480px] p-4">
      <Sidebar label="ナビゲーション" :rail="state.rail ?? false">
        <NavProjects
          :tenant-slug="tenantSlug"
          :projects="state.projects"
          :loading="state.loading ?? false"
          :error="state.error ?? false"
          @retry="onRetry"
        />
      </Sidebar>
    </div>
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

/**
 * レール（アイコンだけ）では Accordion の見出しのアイコンだけが残る。
 * アイコンの無いプロジェクトは頭文字を丸で出す。
 */
export const Rail: Story = {
  render: renderWithState({ projects, rail: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 名前は見た目から消えても読み上げには残る
    await expect(canvas.getByRole('button', { name: 'Design Engineering' })).toBeInTheDocument();
    await expect(canvas.getByText('D')).toBeVisible();
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
    // 見出し横の「+」と空状態カードのボタンの2つが並存する
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

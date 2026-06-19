import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import TaskTablePage from '@/pages/@tenant/projects/@projectKey/tasks/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';

const mockContext = {
  urlPathname: '/tenant-123/projects/ENG/tasks',
  routeParams: { tenant: 'tenant-123', projectKey: 'ENG' },
};

const meta = {
  title: 'Pages/TaskTable',
  component: TaskTablePage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクトタスク一覧の TanStack Table ビュー（モックデータ内蔵）。fetch モック不要。',
      },
    },
  },
  decorators: [
    () => ({
      setup() {
        provide(PAGE_CONTEXT_KEY, mockContext);
      },
      template: '<story />',
    }),
  ],
} satisfies Meta<typeof TaskTablePage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  name: 'タスクテーブル（モックデータ）',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('OAuth 対応を実装する')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('ログイン画面の UI 実装')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('ENG-1')).resolves.toBeInTheDocument();
  },
};

export const Sorting: Story = {
  name: 'ソート操作',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    const titleHeader = await canvas.findByRole('button', { name: /タイトル/ });
    await user.click(titleHeader);
    await expect(canvas.findByText('OAuth 対応を実装する')).resolves.toBeInTheDocument();
  },
};

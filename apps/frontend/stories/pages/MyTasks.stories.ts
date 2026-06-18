import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient } from '@tanstack/vue-query';
import MyTasksPage from '@/pages/@tenant/my-tasks/+Page.vue';

const PAGE_CONTEXT_KEY = 'vike-vue:usePageContext';
// Symbol.for matches @tanstack/vue-query's internal injection key
const VUE_QUERY_CLIENT_KEY = Symbol.for('VUE_QUERY_CLIENT');

const mockContext = {
  urlPathname: '/tenant-123/my-tasks',
  routeParams: { tenant: 'tenant-123' },
};

const jsonResponse = (data: unknown) =>
  new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });

const sampleTasks = [
  {
    id: 'task-1',
    seq_key: 'FE-1',
    title: '仕様書のレビュー',
    priority: 'high',
    soft_deadline: '2026-06-20T00:00:00Z',
    hard_deadline: null,
    is_personal: false,
    project: { id: 'proj-1', name: 'フロントエンド', key: 'FE', is_personal: false },
    status: { id: 's1', name: 'In Progress', color: '#3b82f6', is_done_state: false },
  },
  {
    id: 'task-2',
    seq_key: 'BE-5',
    title: 'APIのドキュメント作成',
    priority: 'medium',
    soft_deadline: null,
    hard_deadline: null,
    is_personal: false,
    project: { id: 'proj-2', name: 'バックエンド', key: 'BE', is_personal: false },
    status: { id: 's2', name: 'Todo', color: '#6b7280', is_done_state: false },
  },
  {
    id: 'task-3',
    seq_key: 'P-1',
    title: '個人メモ',
    priority: 'low',
    soft_deadline: null,
    hard_deadline: null,
    is_personal: true,
    project: { id: 'proj-personal', name: '個人 Inbox', key: 'P', is_personal: true },
    status: { id: 's3', name: 'Todo', color: '#6b7280', is_done_state: false },
  },
];

const meta = {
  title: 'Pages/MyTasks',
  component: MyTasksPage,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component: 'テナント横断のタスク一覧ページ。fetch モックで apiClient を差し替え済み。',
      },
    },
  },
  decorators: [
    () => ({
      setup() {
        const queryClient = new QueryClient({
          defaultOptions: {
            queries: { retry: false, gcTime: 0, staleTime: 0 },
            mutations: { retry: false },
          },
        });
        provide(VUE_QUERY_CLIENT_KEY, queryClient);
        provide(PAGE_CONTEXT_KEY, mockContext);
      },
      template: '<story />',
    }),
  ],
} satisfies Meta<typeof MyTasksPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const WithTasks: Story = {
  name: 'タスクあり（個人 + プロジェクト）',
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockResolvedValue(jsonResponse({ tasks: sampleTasks }));
    return () => {
      globalThis.fetch = original;
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('仕様書のレビュー')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('APIのドキュメント作成')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('個人メモ')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('個人 Inbox')).resolves.toBeInTheDocument();
  },
};

export const Empty: Story = {
  name: 'タスクなし',
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockResolvedValue(jsonResponse({ tasks: [] }));
    return () => {
      globalThis.fetch = original;
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクがありません')).resolves.toBeInTheDocument();
  },
};

export const ApiError: Story = {
  name: 'API エラー',
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockRejectedValue(new TypeError('Failed to fetch'));
    return () => {
      globalThis.fetch = original;
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('タスクの読み込みに失敗しました')).resolves.toBeInTheDocument();
  },
};

export const Loading: Story = {
  name: 'ロード中',
  beforeEach() {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation(() => new Promise(() => {}));
    return () => {
      globalThis.fetch = original;
    };
  },
};

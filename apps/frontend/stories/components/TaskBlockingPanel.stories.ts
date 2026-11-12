import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import TaskBlockingPanel from '@/components/tasks/TaskBlockingPanel.vue';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';
const SELF_UUID = '00000000-0000-4000-8000-000000000001';

const status = (id: string, name: string, color: string, done: boolean, position: number) => ({
  id,
  name,
  color,
  position,
  is_default: position === 0,
  is_done_state: done,
  is_default_done: done,
  project_id: PROJECT_UUID,
  created_at: '2026-07-15T00:00:00Z',
});

const statuses = [
  status('status-todo', '未着手', '#94a3b8', false, 0),
  status('status-progress', '進行中', '#3b82f6', false, 1),
  status('status-done', '完了', '#22c55e', true, 2),
];

const task = (seq: number, title: string, statusId: string) => ({
  id: `00000000-0000-4000-8000-0000000001${String(seq).padStart(2, '0')}`,
  project_id: PROJECT_UUID,
  seq_id: seq,
  title,
  status_id: statusId,
  priority: 'Medium',
  progress_pct: 0,
  is_archived: false,
  assignees: [],
  labels: [],
  created_at: '2026-07-01T00:00:00Z',
  updated_at: '2026-07-10T00:00:00Z',
});

const entry = (seq: number, title: string, statusId: string) => ({
  ...task(seq, title, statusId),
  relation_id: `00000000-0000-4000-8000-0000000002${String(seq).padStart(2, '0')}`,
});

const blockedBy = [
  entry(12, 'OAuth のクライアント登録を済ませる', 'status-progress'),
  entry(14, 'リダイレクト URI の許可リストを決める', 'status-todo'),
  entry(9, 'IdP の選定', 'status-done'),
];
const blocks = [entry(20, 'ログイン画面を差し替える', 'status-todo')];

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

/** relations と検索を返す fetch モック。`conflict` のとき POST は 409 */
function mockFetch(
  relations: { blocks: unknown[]; blocked_by: unknown[] },
  options: { conflict?: boolean } = {},
) {
  return () => {
    const original = globalThis.fetch;
    globalThis.fetch = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      if (method === 'POST' && pathname.endsWith('/relations')) {
        return options.conflict
          ? jsonResponse({ message: 'conflict' }, 409)
          : jsonResponse({ relation_id: 'new' }, 201);
      }
      if (method === 'GET' && pathname.endsWith('/relations')) {
        return jsonResponse({ parent: null, subtasks: [], ...relations });
      }
      if (method === 'GET' && pathname.endsWith('/tasks/search')) {
        const tasks = [
          task(31, 'トークンの更新処理を実装する', 'status-todo'),
          task(32, 'トークンの失効 API を足す', 'status-progress'),
        ];
        return jsonResponse({ tasks, total: tasks.length });
      }
      return jsonResponse({ message: 'not found' }, 404);
    });
    return () => {
      globalThis.fetch = original;
    };
  };
}

const meta = {
  title: 'Components/Tasks/TaskBlockingPanel',
  component: TaskBlockingPanel,
  tags: ['autodocs'],
  args: {
    tenantId: TENANT_UUID,
    projectId: PROJECT_UUID,
    taskId: 'TASK-1',
    taskUuid: SELF_UUID,
    statuses,
    projectKey: 'TASK',
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'タスク詳細の「ブロック関係」欄。ブロックされている / ブロックしている の一覧、未完了のブロッカー件数、相手の追加（検索）と解除。fetch モックで検証。',
      },
    },
  },
  decorators: [
    () => ({
      setup() {
        provide(
          VUE_QUERY_CLIENT,
          new QueryClient({
            defaultOptions: {
              queries: { retry: false, gcTime: 0, staleTime: 0 },
              mutations: { retry: false },
            },
          }),
        );
      },
      template: '<div class="max-w-2xl"><story /></div>',
    }),
  ],
} satisfies Meta<typeof TaskBlockingPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const NoRelations: Story = {
  name: '関係なし',
  beforeEach: mockFetch({ blocks: [], blocked_by: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('ブロックされているタスクはありません')).resolves.toBeVisible();
    await expect(canvas.getByText('ブロックしているタスクはありません')).toBeVisible();
    await expect(canvasElement.querySelector('[data-task-blocking-open-count]')).toBeNull();
  },
};

export const BlockedBy: Story = {
  name: 'ブロックされている（未完了 2 件・完了 1 件）',
  beforeEach: mockFetch({ blocks: [], blocked_by: blockedBy }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('OAuth のクライアント登録を済ませる')).resolves.toBeVisible();
    await expect(canvas.getByText('未完了のブロッカー 2 件')).toBeVisible();
    await expect(canvas.getByText('ブロックしているタスクはありません')).toBeVisible();
  },
};

export const Blocks: Story = {
  name: 'ブロックしている',
  beforeEach: mockFetch({ blocks, blocked_by: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('ログイン画面を差し替える')).resolves.toBeVisible();
    await expect(canvasElement.querySelector('[data-task-blocking-open-count]')).toBeNull();
  },
};

export const BothDirections: Story = {
  name: 'ブロックされている + ブロックしている',
  beforeEach: mockFetch({ blocks, blocked_by: blockedBy }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('OAuth のクライアント登録を済ませる')).resolves.toBeVisible();
    await expect(canvas.getByText('ログイン画面を差し替える')).toBeVisible();
    await expect(canvas.getByText('未完了のブロッカー 2 件')).toBeVisible();
  },
};

export const Adding: Story = {
  name: '追加中（相手を検索）',
  beforeEach: mockFetch({ blocks, blocked_by: blockedBy }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: 'ブロックするタスクを追加' }));
    await user.type(canvas.getByLabelText('ブロックするタスクを検索'), 'トークン');
    await expect(canvas.findByText('トークンの更新処理を実装する')).resolves.toBeVisible();
  },
};

export const AddConflict: Story = {
  name: '追加が 409（循環・重複のエラー表示）',
  beforeEach: mockFetch({ blocks, blocked_by: blockedBy }, { conflict: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await user.click(await canvas.findByRole('button', { name: 'ブロックされるタスクを追加' }));
    await user.type(canvas.getByLabelText('ブロックされるタスクを検索'), 'トークン');
    await user.click(await canvas.findByRole('button', { name: /トークンの更新処理を実装する/ }));
    await expect(canvas.findByRole('alert')).resolves.toHaveTextContent(
      '循環する関係や同じ関係は設定できません',
    );
  },
};

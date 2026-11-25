import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
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

/** 図に出す日付。プレビューの「今日」は 2026-07-15 に固定されている */
type Dates = {
  planned_start_on?: string | null;
  soft_deadline?: string | null;
  hard_deadline?: string | null;
};
const day = (date: string) => `${date}T00:00:00.000Z`;

const task = (seq: number, title: string, statusId: string, dates: Dates = {}) => ({
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
  ...dates,
});

const entry = (seq: number, title: string, statusId: string, dates: Dates = {}) => ({
  ...task(seq, title, statusId, dates),
  relation_id: `00000000-0000-4000-8000-0000000002${String(seq).padStart(2, '0')}`,
});

const blockedBy = [
  // 期限（7/21）がこのタスクの開始（7/16）より後 = 点線と注記
  entry(12, 'OAuth のクライアント登録を済ませる', 'status-progress', {
    planned_start_on: '2026-07-13',
    hard_deadline: day('2026-07-21'),
  }),
  // 期限だけ = 期限の日の点
  entry(14, 'リダイレクト URI の許可リストを決める', 'status-todo', {
    soft_deadline: day('2026-07-15'),
  }),
  entry(9, 'IdP の選定', 'status-done', {
    planned_start_on: '2026-07-08',
    hard_deadline: day('2026-07-10'),
  }),
];
// 開始も期限も無い = 今日の 1 日の棒（仮の扱い）
const blocks = [entry(20, 'ログイン画面を差し替える', 'status-todo')];

/** このタスク自身（図のまんなかの行） */
const self = {
  ...task(1, 'OAuth ログインを実装する', 'status-progress', {
    planned_start_on: '2026-07-16',
    soft_deadline: day('2026-07-22'),
    hard_deadline: day('2026-07-24'),
  }),
  id: SELF_UUID,
  custom_field_values: [],
  reactions: [],
  attachments: [],
};

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
      if (method === 'DELETE' && pathname.includes('/relations/')) {
        return new Response(null, { status: 204 });
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
    task: self as never,
    statuses,
    projectKey: 'TASK',
    tenantDisplayId: 'acme',
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'タスク詳細の「ブロック関係」欄。小さなガント（待たせている相手 → このタスク → 待っている相手）と、日付の食い違いの注記、未完了のブロッカー件数、相手の追加（検索）と解除。fetch モックで検証。',
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

/** メニュー（top layer）から項目を選ぶ */
async function pickFromMenu(
  canvasElement: HTMLElement,
  user: ReturnType<typeof userEvent.setup>,
  trigger: string,
  item: string,
) {
  await user.click(await within(canvasElement).findByRole('button', { name: trigger }));
  await user.click(await within(document.body).findByRole('menuitem', { name: item }));
}

const labelRoles = (canvasElement: HTMLElement) =>
  [...canvasElement.querySelectorAll('[data-blocking-role]')].map((row) =>
    row.getAttribute('data-blocking-role'),
  );

export const NoRelations: Story = {
  name: '関係なし',
  beforeEach: mockFetch({ blocks: [], blocked_by: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('ブロック関係はありません')).resolves.toBeVisible();
    await expect(canvasElement.querySelector('[data-task-blocking-open-count]')).toBeNull();
    await expect(canvasElement.querySelector('[data-task-blocking-chart]')).toBeNull();
    // 追加の入口は 1 つ
    await expect(canvas.getAllByRole('button', { name: '関係を追加' })).toHaveLength(1);
  },
};

export const BlockedBy: Story = {
  name: 'ブロックされている（未完了 2 件・完了 1 件）',
  beforeEach: mockFetch({ blocks: [], blocked_by: blockedBy }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('OAuth のクライアント登録を済ませる')).resolves.toBeVisible();
    await expect(canvas.getByText('未完了のブロッカー 2 件')).toBeVisible();
    // 待たせている相手 → このタスク の順。このタスクは太字
    await expect(labelRoles(canvasElement)).toEqual([
      'blocked_by',
      'blocked_by',
      'blocked_by',
      'self',
    ]);
    await expect(canvas.getByText('OAuth ログインを実装する')).toHaveClass('font-semibold');
    // 相手の期限がこのタスクの開始より後のときだけ、図の下に注記を出す
    await expect(
      canvas.getByText('点線：TASK-12 の期限（7/21）が、このタスクの開始（7/16）より後'),
    ).toBeVisible();
    // 図は client の mount の後に描かれる
    await waitFor(() =>
      expect(canvasElement.querySelector('[data-task-blocking-chart] svg')).not.toBeNull(),
    );
  },
};

export const Blocks: Story = {
  name: 'ブロックしている（日付の無い相手は今日の 1 日の棒）',
  beforeEach: mockFetch({ blocks, blocked_by: [] }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('ログイン画面を差し替える')).resolves.toBeVisible();
    await expect(canvasElement.querySelector('[data-task-blocking-open-count]')).toBeNull();
    await expect(labelRoles(canvasElement)).toEqual(['self', 'blocks']);
    // 日付が足りない依存は食い違いにしない
    await expect(canvasElement.querySelector('[data-task-blocking-notes]')).toBeNull();
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
    await expect(labelRoles(canvasElement)).toEqual([
      'blocked_by',
      'blocked_by',
      'blocked_by',
      'self',
      'blocks',
    ]);
    // ラベルの行の高さは図の行（32px）と同じ
    const rows = [...canvasElement.querySelectorAll<HTMLElement>('[data-blocking-role]')];
    for (const row of rows) await expect(row.getBoundingClientRect().height).toBe(32);
  },
};

export const LabelHoverCard: Story = {
  name: 'ラベルのホバーカード（キーボードのフォーカスでも開く）',
  beforeEach: mockFetch({ blocks, blocked_by: blockedBy }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const label = (await canvas.findByText('OAuth のクライアント登録を済ませる')).closest(
      'button',
    )!;
    // キーボードで辿り着く（:focus-visible）。先頭の「関係を追加」の次がこのラベル
    const user = userEvent.setup();
    await user.tab();
    await user.tab();
    await expect(label).toHaveFocus();
    const card = await within(document.body).findByRole('dialog', { name: 'TASK-12 のタスク' });
    // カードは現れきるまで透明なので、見えるまで待つ
    await waitFor(() => expect(within(card).getByText('TASK-12')).toBeVisible());
    await expect(within(card).getByText('OAuth のクライアント登録を済ませる')).toBeVisible();
    await user.keyboard('{Escape}');
    await waitFor(() => expect(within(document.body).queryByRole('dialog')).toBeNull());
  },
};

export const RemoveRelation: Story = {
  name: '関係を外す（ラベルの行のメニュー）',
  beforeEach: mockFetch({ blocks, blocked_by: blockedBy }),
  play: async ({ canvasElement }) => {
    const user = userEvent.setup();
    await within(canvasElement).findByText('ログイン画面を差し替える');
    await pickFromMenu(canvasElement, user, 'TASK-20 との関係の操作', '関係を外す');
    await waitFor(() =>
      expect(
        (globalThis.fetch as ReturnType<typeof fn>).mock.calls.some(
          ([req]) => typeof req !== 'string' && (req as Request).method === 'DELETE',
        ),
      ).toBe(true),
    );
  },
};

export const Adding: Story = {
  name: '追加中（向きを選んでから相手を検索）',
  beforeEach: mockFetch({ blocks, blocked_by: blockedBy }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await canvas.findByText('ログイン画面を差し替える');
    await pickFromMenu(canvasElement, user, '関係を追加', 'ブロックするタスクを追加');
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
    await canvas.findByText('ログイン画面を差し替える');
    await pickFromMenu(canvasElement, user, '関係を追加', 'ブロックされるタスクを追加');
    await user.type(canvas.getByLabelText('ブロックされるタスクを検索'), 'トークン');
    await user.click(await canvas.findByRole('button', { name: /トークンの更新処理を実装する/ }));
    await expect(canvas.findByRole('alert')).resolves.toHaveTextContent(
      '循環する関係や同じ関係は設定できません',
    );
  },
};

import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, waitFor, within } from 'storybook/test';

import TaskComments from '@/components/tasks/TaskComments.vue';
import type { CommentThread } from '@/composables/useTaskComments';
import type { ActivityItem } from '@/lib/task-activity';

/**
 * タスク詳細の右列（幅 304px）の実ブラウザでの挙動。高さを固定して、スクロールの
 * 位置を確かめる（単体テストの DOM にはレイアウトが無い）。
 */
const yupix = { id: 'user-1', name: 'yupix' };
const tanaka = { id: 'user-2', name: '田中' };

function status(id: string, created_at: string, to: string): ActivityItem {
  return {
    id,
    created_at,
    event_type: 'status_changed',
    payload: { from: 'Todo', to },
    user: yupix,
  } as ActivityItem;
}

function comment(id: string, created_at: string, body: string): CommentThread {
  return {
    id,
    body,
    is_deleted: false,
    created_at,
    updated_at: created_at,
    user: tanaka,
    replies: [],
  } as CommentThread;
}

const meta = {
  title: 'Tasks/TaskActivityColumn',
  component: TaskComments,
  render: (args) => ({
    components: { TaskComments },
    setup: () => ({ args }),
    template: `
      <div class="flex h-[640px] w-[304px] flex-col border-l px-4">
        <TaskComments v-bind="args" />
      </div>
    `,
  }),
  args: {
    threads: [],
    onSubmit: async () => true,
    onUpdate: async () => true,
    onDelete: async () => true,
  },
} satisfies Meta<typeof TaskComments>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 束ねられないよう 1 時間ずつ離す。index が小さいほど新しい（API の順） */
const longHistory = Array.from({ length: 40 }, (_, index) =>
  status(
    `long-${index}`,
    new Date(Date.UTC(2026, 5, 20, 12) - index * 3_600_000).toISOString(),
    `状態 ${40 - index}`,
  ),
);

export const LongHistory: Story = {
  name: '長い履歴でも先頭に届く',
  args: {
    activities: longHistory,
    threads: [comment('c-1', '2026-06-20T06:30:00Z', '途中のコメントです')],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('状態 40')).resolves.toBeInTheDocument();
    const feed = canvasElement.querySelector('[data-activity-feed]') as HTMLElement;
    await expect(feed.scrollHeight).toBeGreaterThan(feed.clientHeight);

    // 開いた時は最新（下端）にいる
    await waitFor(() =>
      expect(feed.scrollHeight - feed.scrollTop - feed.clientHeight).toBeLessThan(2),
    );

    // 上端までスクロールすると、最も古い行が枠の中に入る
    feed.scrollTop = 0;
    const oldest = canvas.getByText('状態 1').getBoundingClientRect();
    const box = feed.getBoundingClientRect();
    await expect(oldest.top).toBeGreaterThanOrEqual(box.top);
    await expect(oldest.bottom).toBeLessThanOrEqual(box.bottom);
  },
};

export const ShortHistory: Story = {
  name: '少ないときは入力欄の側へ寄せる',
  args: {
    activities: [status('h1', '2026-06-20T01:00:00Z', 'Done')],
    threads: [comment('c-1', '2026-06-20T02:00:00Z', 'コメントです')],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('コメントです')).resolves.toBeInTheDocument();
    const feed = canvasElement.querySelector('[data-activity-feed]') as HTMLElement;
    const last = canvasElement.querySelector('[data-task-comment]') as HTMLElement;
    await expect(feed.scrollHeight).toBe(feed.clientHeight);
    await expect(
      feed.getBoundingClientRect().bottom - last.getBoundingClientRect().bottom,
    ).toBeLessThan(16);
  },
};

export const WorstCase: Story = {
  name: '長い文字列でも列からはみ出さない',
  args: {
    lookups: {
      statuses: [{ name: 'AwaitingAccessibilityVerification', color: '#594867' }],
      members: [{ id: 'u1', username: 'bartholomew.fitzgerald-northwind-industries' }],
      labels: [{ id: 'l1', name: 'needs-design-review-before-release', color: '#e11d48' }],
    },
    activities: [
      {
        id: 'p1',
        created_at: '2026-06-20T01:05:00Z',
        event_type: 'forge_pull_request_linked',
        payload: {
          number: 123456,
          title:
            'fix(frontend): サインインと新規登録の空の紺色の面を消して、カードの幅をパスワード再設定と揃える',
          html_url: 'https://example.com/pr',
        },
        user: { id: 'u2', name: 'Aleksandra Wiśniewska-Kowalczyk' },
      },
      {
        id: 'l1',
        created_at: '2026-06-20T01:04:00Z',
        event_type: 'label_added',
        payload: { label_id: 'l1', name: 'needs-design-review-before-release' },
        user: { id: 'u2', name: 'Aleksandra Wiśniewska-Kowalczyk' },
      },
      {
        id: 'a1',
        created_at: '2026-06-20T01:03:00Z',
        event_type: 'assignee_added',
        payload: { user_id: 'u1' },
        user: { id: 'u2', name: 'Aleksandra Wiśniewska-Kowalczyk' },
      },
      {
        id: 's1',
        created_at: '2026-06-20T01:02:00Z',
        event_type: 'status_changed',
        payload: {
          from: 'AwaitingAccessibilityVerification',
          to: 'AwaitingAccessibilityVerification',
        },
        user: { id: 'u2', name: 'Aleksandra Wiśniewska-Kowalczyk' },
      },
    ] as ActivityItem[],
    threads: [
      comment(
        'c-1',
        '2026-06-20T02:00:00Z',
        'bartholomew.fitzgerald@northwind-industries-holdings.example.com へ送ります',
      ),
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText(/needs-design-review/)).resolves.toBeInTheDocument();
    const feed = canvasElement.querySelector('[data-activity-feed]') as HTMLElement;
    await expect(feed.scrollWidth).toBeLessThanOrEqual(feed.clientWidth);
  },
};

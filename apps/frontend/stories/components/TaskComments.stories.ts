import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, waitFor, within } from 'storybook/test';

import TaskActivityFeed from '@/components/tasks/TaskActivityFeed.vue';
import TaskComments from '@/components/tasks/TaskComments.vue';
import type { CommentThread } from '@/composables/useTaskComments';
import type { ActivityItem } from '@/lib/task-activity';

/**
 * アクティビティ列のスクロール領域の実ブラウザでの挙動。happy-dom にはレイアウトが無いので、
 * 少ないときの下寄せと、多いときに先頭まで届くことはここで固定する。
 */
const user = { id: 'user-1', name: '田中太郎' };

function threads(count: number): CommentThread[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `c-${i}`,
    body: `コメント ${i + 1}`,
    is_deleted: false,
    created_at: '2026-08-19T00:00:00Z',
    updated_at: '2026-08-19T00:00:00Z',
    user,
    replies: [],
  }));
}

function activities(count: number): ActivityItem[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `a-${i}`,
    event_type: 'label_added',
    payload: { name: `ラベル ${i + 1}` },
    created_at: '2026-08-19T00:00:00Z',
    user,
  })) as ActivityItem[];
}

const meta = {
  title: 'Tasks/TaskComments',
  component: TaskComments,
  args: {
    threads: [],
    onSubmit: fn(async () => true),
    onUpdate: fn(async () => true),
    onDelete: fn(async () => true),
  },
} satisfies Meta<typeof TaskComments>;

export default meta;
type Story = StoryObj<typeof meta>;

/** タスク詳細の右の欄と同じく、高さの決まった列に入れる */
function inColumn(activityCount: number) {
  return (args: Record<string, unknown>) => ({
    components: { TaskActivityFeed, TaskComments },
    setup: () => ({ args, items: activities(activityCount) }),
    template: `
      <div style="display: flex; flex-direction: column; width: 19rem; height: 480px; padding: 0 1rem">
        <TaskComments v-bind="args">
          <template #before-list><TaskActivityFeed :activities="items" /></template>
        </TaskComments>
      </div>`,
  });
}

function scroller(canvasElement: HTMLElement) {
  return canvasElement.querySelector<HTMLElement>('[data-task-comments] .overflow-y-auto')!;
}

export const ManyItems: Story = {
  name: '多いときは先頭まで届く',
  args: { threads: threads(12) },
  render: inColumn(12),
  play: async ({ canvasElement }) => {
    const el = scroller(canvasElement);
    const oldest = canvasElement.querySelector('[data-task-activity-feed] li')!;
    await waitFor(() => expect(el.scrollHeight).toBeGreaterThan(el.clientHeight));

    el.scrollTop = 0;
    await expect(oldest.getBoundingClientRect().top).toBeGreaterThanOrEqual(
      el.getBoundingClientRect().top,
    );
  },
};

export const FewItems: Story = {
  name: '少ないときは下端に寄せる',
  args: { threads: threads(1) },
  render: inColumn(1),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const el = scroller(canvasElement);
    const comment = await canvas.findByText('コメント 1');
    const card = comment.closest('li')!;

    const bottomGap = el.getBoundingClientRect().bottom - card.getBoundingClientRect().bottom;
    // 下の余白は領域の py-3（12px）だけ
    await expect(bottomGap).toBeLessThanOrEqual(13);
    await expect(el.scrollHeight).toBe(el.clientHeight);
  },
};

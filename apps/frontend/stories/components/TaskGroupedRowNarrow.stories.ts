import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, waitFor } from 'storybook/test';

import TaskGroupedRow from '@/components/tasks/TaskGroupedRow.vue';
import type { components } from '@/generated/api';

type TaskResponse = components['schemas']['TaskResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];

const status: StatusResponse = {
  id: 'status-todo',
  name: 'Todo',
  color: '#94a3b8',
  position: 0,
  is_default: true,
  is_done_state: false,
  is_default_done: false,
  project_id: 'project-1',
  created_at: '2026-06-01T00:00:00Z',
};

/** 長いキー・5 桁の番号・期限・BOT が先頭の担当。2 段目がいちばん詰まる組 */
const task: TaskResponse = {
  id: 'task-1',
  project_id: 'project-1',
  seq_id: 12345,
  title: 'ログイン画面のエラー表示を直す',
  status_id: status.id,
  priority: 'High',
  progress_pct: 0,
  is_archived: false,
  soft_deadline: '2026-07-20T00:00:00Z',
  assignees: [
    { role: 'assignee', user: { id: 'bot-1', username: 'release-bot', account_kind: 'bot' } },
    { role: 'assignee', user: { id: 'user-1', username: 'yupix' } },
  ],
  labels: [{ id: 'label-1', name: 'UX改善', color: '#10b981' }] as TaskResponse['labels'],
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-01T00:00:00Z',
};

const meta = {
  title: 'Components/Tasks/TaskGroupedRowNarrow',
  component: TaskGroupedRow,
  args: {
    projectKey: 'ABCDEFGH',
    task,
    statuses: [status],
    projectLabels: [],
    members: [],
    onComment: async () => true,
  },
  // 幅 320px の画面から左右 16px の余白を除いた幅。列の切り替えは一覧（@container）の幅で決まる
  decorators: [
    () => ({ template: '<div class="@container" style="width: 288px"><story /></div>' }),
  ],
} satisfies Meta<typeof TaskGroupedRow>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 要素の中心にある要素（elementFromPoint） */
function hitAtCenter(el: Element): Element | null {
  const rect = el.getBoundingClientRect();
  return document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
}

/**
 * 狭い幅の当たり判定（TASK-524）。ID のコピーのボタンは自分の列からはみ出さず、
 * 期限・優先度・担当（BOT のアバターを含む）の上はタイトルのボタン（after の的）に当たる。
 */
export const HitArea: Story = {
  play: async ({ canvasElement }) => {
    const summary = await waitFor(() => {
      const el = canvasElement.querySelector<HTMLElement>('[data-row-summary]');
      if (!el || el.getBoundingClientRect().width === 0) throw new Error('2 段目がまだ出ていない');
      return el;
    });
    const title = canvasElement.querySelector('[data-task-title-open]')!;
    const copy = summary.querySelector<HTMLElement>('[data-task-id-copy]')!;
    // コピーの的は ID の列（期限より左）に収まる
    await expect(copy.getBoundingClientRect().right).toBeLessThanOrEqual(
      copy.parentElement!.getBoundingClientRect().right + 0.5,
    );
    await expect(hitAtCenter(copy)?.closest('[data-task-id-copy]')).toBe(copy);
    for (const text of summary.querySelectorAll(':scope > span')) {
      await expect(hitAtCenter(text)?.closest('[data-task-title-open]')).toBe(title);
    }
    // BOT のアバター（根が relative）の上もタイトルに当たる
    const bot = summary.querySelector('[aria-label="BOT アカウント"]')!;
    await expect(hitAtCenter(bot)?.closest('[data-task-title-open]')).toBe(title);
  },
};

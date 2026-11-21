import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor } from 'storybook/test';

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

const CHECKLIST = '- [x] 設計\n- [ ] 実装';
const label = (id: string, name: string, color: string) => ({ id, name, color });
const person = (id: string, username: string) => ({ role: 'assignee', user: { id, username } });
const bot = {
  role: 'assignee',
  user: { id: 'bot-1', username: 'release-bot', account_kind: 'bot' },
};

type RowCase = {
  /** 行の名前（a〜h） */
  name: string;
  projectKey: string;
  seqId: number;
  checklist?: boolean;
  labels?: ReturnType<typeof label>[];
  assignees?: unknown[];
  depth?: 0 | 1;
  noDeadline?: boolean;
};

/** 2 段目の左の群が詰まる境の行。総当たりではなく、それぞれ 1 つの境を受け持つ */
const ROWS: RowCase[] = [
  { name: 'a いちばん短い', projectKey: 'AB', seqId: 1 },
  {
    name: 'b 達成率とラベル',
    projectKey: 'TASK',
    seqId: 123,
    checklist: true,
    labels: [label('l1', 'バックエンド改善', '#3b82f6')],
    assignees: [person('u1', 'yupix')],
  },
  {
    name: 'c ラベルだけ（+2）',
    projectKey: 'TASK',
    seqId: 123,
    labels: [
      label('l1', 'UX改善', '#10b981'),
      label('l2', 'バグ', '#ef4444'),
      label('l3', 'フロントエンド', '#3b82f6'),
    ],
    assignees: [person('u1', 'yupix'), person('u2', 'sousuke')],
  },
  {
    name: 'd 288px でちょうど入る',
    projectKey: 'TASK',
    seqId: 12345,
    checklist: true,
    assignees: [person('u1', 'yupix')],
  },
  {
    name: 'e いちばん長い ID と BOT',
    projectKey: 'ABCDEFGHIJ',
    seqId: 12345,
    checklist: true,
    labels: [label('l1', 'UX改善', '#10b981')],
    assignees: [bot, person('u1', 'yupix')],
  },
  { name: 'f 6 桁', projectKey: 'ABCDEFGHIJ', seqId: 123456 },
  {
    name: 'g サブタスクの行',
    projectKey: 'TASK',
    seqId: 12345,
    checklist: true,
    labels: [label('l1', 'UX改善', '#10b981')],
    assignees: [person('u1', 'yupix')],
    depth: 1,
  },
  { name: 'h 期限なし', projectKey: 'TASK', seqId: 123, noDeadline: true },
];

/** 一覧の幅。320 / 390 / 480 / 768px の画面から左右 16px の余白を除いた幅 */
const WIDTHS = [288, 358, 448, 736];

function taskOf(row: RowCase): TaskResponse {
  return {
    id: `task-${row.name}`,
    project_id: 'project-1',
    seq_id: row.seqId,
    title: 'ログイン画面のエラー表示を直す',
    description: row.checklist ? CHECKLIST : null,
    status_id: status.id,
    priority: 'High',
    progress_pct: 0,
    is_archived: false,
    soft_deadline: row.noDeadline ? null : '2026-07-20T00:00:00Z',
    assignees: (row.assignees ?? []) as TaskResponse['assignees'],
    labels: (row.labels ?? []) as TaskResponse['labels'],
    created_at: '2026-06-01T00:00:00Z',
    updated_at: '2026-06-01T00:00:00Z',
  };
}

const onOpen = fn();
const onSelect = fn();

/** 枠の幅ごとに行を並べる。列の切り替えは一覧（@container）の幅で決まるので、画面幅に依らない */
function renderRows(widths: number[], rows: RowCase[]) {
  return () => ({
    components: { TaskGroupedRow },
    setup: () => ({
      frames: widths.map((width) => ({
        width,
        rows: rows.map((row) => ({ row, task: taskOf(row) })),
      })),
      status,
      onOpen,
      onSelect,
    }),
    template: `
      <div class="flex flex-col gap-4">
        <div
          v-for="frame in frames"
          :key="frame.width"
          class="@container"
          :style="{ width: frame.width + 'px' }"
          :data-frame="frame.width"
        >
          <TaskGroupedRow
            v-for="{ row, task } in frame.rows"
            :key="row.name"
            :data-case="row.name"
            :project-key="row.projectKey"
            :task="task"
            :statuses="[status]"
            :project-labels="[]"
            :members="[]"
            :depth="row.depth ?? 0"
            :on-comment="async () => true"
            @open="onOpen"
            @select="onSelect"
          />
        </div>
      </div>`,
  });
}

const meta = {
  title: 'Components/Tasks/TaskGroupedRowNarrow',
  // 枠ごとに行を並べて描く。部品の args は使わない
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

/** 要素を画面の中へ寄せてから、中心にある要素を引く（画面幅に依らない） */
function hitAtCenter(el: Element): Element | null {
  el.scrollIntoView({ block: 'center', inline: 'center' });
  const rect = el.getBoundingClientRect();
  return document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
}

const within = (inner: DOMRect, outer: DOMRect) =>
  inner.left >= outer.left - 0.5 &&
  inner.right <= outer.right + 0.5 &&
  inner.top >= outer.top - 0.5 &&
  inner.bottom <= outer.bottom + 0.5;

async function summaries(canvasElement: HTMLElement, count: number) {
  return waitFor(() => {
    const els = [...canvasElement.querySelectorAll<HTMLElement>('[data-row-summary]')];
    if (els.length !== count || els.some((el) => el.getBoundingClientRect().width === 0))
      throw new Error('2 段目がまだ出ていない');
    return els;
  });
}

/**
 * 狭い幅の 2 段目の並びと当たり判定（TASK-524）。どの枠・どの行でも次が成り立つ。
 * ID のコピーの的は左の群を越えず、番号は欠けない。達成率とラベルは丸ごと見えるか丸ごと隠れる。
 * 期限・優先度・担当（BOT の札を含む）の上は詳細を開き、ID・矢印・丸はそれぞれの操作に当たる。
 */
export const NarrowLayout: Story = {
  render: renderRows(WIDTHS, ROWS),
  play: async ({ canvasElement }) => {
    const all = await summaries(canvasElement, WIDTHS.length * ROWS.length);
    for (const summary of all) {
      const row = summary.closest<HTMLElement>('[data-case]')!;
      const where = `${row.closest('[data-frame]')!.getAttribute('data-frame')}px ${row.dataset.case}`;
      const left = summary.querySelector<HTMLElement>('[data-row-summary-left]')!;
      const leftBox = left.getBoundingClientRect();
      const copy = left.querySelector<HTMLElement>('[data-task-id-copy]')!;
      const copyBox = copy.getBoundingClientRect();
      const number = copy.querySelector<HTMLElement>('[data-task-id-number]')!;

      // C1: コピーの的は左の群の中
      await expect(within(copyBox, leftBox), `${where}: ID が群を越える`).toBe(true);
      // C2: 番号は全部見える
      await expect(
        within(number.getBoundingClientRect(), copyBox),
        `${where}: 番号がボタンを越える`,
      ).toBe(true);
      await expect(number.scrollWidth, `${where}: 番号が欠ける`).toBeLessThanOrEqual(
        number.clientWidth,
      );
      // C3: 達成率とラベルは丸ごと見えるか、群の下（見えない 2 行目）にある
      for (const part of left.querySelectorAll('[data-checklist-progress], [data-row-label]')) {
        const box = part.getBoundingClientRect();
        await expect(
          within(box, leftBox) || box.top >= leftBox.bottom - 0.5,
          `${where}: ${part.textContent} が半端に出る`,
        ).toBe(true);
      }

      // C5: 期限・優先度・担当と BOT の札は詳細を開く
      const title = row.querySelector('[data-task-title-open]')!;
      for (const target of [
        ...summary.querySelectorAll(':scope > span'),
        ...summary.querySelectorAll('[aria-label="BOT アカウント"]'),
      ]) {
        await expect(
          hitAtCenter(target)?.closest('[data-task-title-open]'),
          `${where}: ${target.textContent} の上で詳細が開かない`,
        ).toBe(title);
      }
      // C6・C7: ID・矢印・丸はそれぞれの操作
      await expect(hitAtCenter(copy)?.closest('[data-task-id-copy]'), where).toBe(copy);
      const toggle = row.querySelector('button[aria-label="サブタスクを展開"]')!;
      await expect(hitAtCenter(toggle)?.closest('button'), where).toBe(toggle);
      const statusButton = row.querySelector('button[aria-label^="ステータス"]')!;
      await expect(hitAtCenter(statusButton)?.closest('button'), where).toBe(statusButton);
    }
  },
};

/**
 * ID を押した前後で、左の群の並びが動かない（C8）。押してもコピーだけで、詳細も選択も起きない。
 * headless の Chromium はクリップボードへの書き込みを拒むことがあるので、成功でも失敗でも通る形で見る
 */
export const Copied: Story = {
  render: renderRows([358], [ROWS[1]]),
  play: async ({ canvasElement }) => {
    onOpen.mockClear();
    onSelect.mockClear();
    const [summary] = await summaries(canvasElement, 1);
    const left = summary.querySelector<HTMLElement>('[data-row-summary-left]')!;
    const layout = () =>
      [...left.querySelectorAll<HTMLElement>(':scope > *')]
        .map((el) => ({ el, box: el.getBoundingClientRect() }))
        .filter(({ box }) => box.top < left.getBoundingClientRect().bottom - 0.5)
        .map(({ el, box }) => ({ part: el.textContent, left: box.left, width: box.width }));
    const before = layout();
    const copy = left.querySelector<HTMLElement>('[data-task-id-copy]')!;
    const idle = copy.getAttribute('aria-label');

    await userEvent.click(copy);
    await waitFor(() => expect(copy.getAttribute('aria-label')).not.toBe(idle));

    await expect(layout()).toEqual(before);
    await expect(copy.querySelector('svg')).toBeNull();
    await expect(onOpen).not.toHaveBeenCalled();
    await expect(onSelect).not.toHaveBeenCalled();
  },
};

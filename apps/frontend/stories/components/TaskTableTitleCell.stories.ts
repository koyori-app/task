import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, waitFor } from 'storybook/test';

import TaskTableTitleCell from '@/components/tasks/TaskTableTitleCell.vue';

const ROWS = [
  { seqId: 1, title: 'OAuth 対応を実装する', priority: 'High' as const },
  { seqId: 12345, title: 'ログイン画面のエラー表示を直す', priority: 'Medium' as const },
];

/** 幅 320px の表に 2 行。狭い形は prop で決めるので、画面幅に依らない */
function renderTable(narrow: boolean) {
  return () => ({
    components: { TaskTableTitleCell },
    setup: () => ({ rows: ROWS, narrow }),
    template: `
      <table style="width: 320px; border-collapse: collapse">
        <tbody>
          <tr v-for="row in rows" :key="row.seqId" :data-row="row.seqId">
            <td style="padding: 4px 8px; border-bottom: 1px solid #ccc">
              <TaskTableTitleCell
                tenant-display-id="acme"
                project-key="ENG"
                :seq-id="row.seqId"
                :title="row.title"
                :priority="row.priority"
                :narrow="narrow"
              />
            </td>
          </tr>
        </tbody>
      </table>`,
  });
}

const meta = {
  title: 'Components/Tasks/TaskTableTitleCell',
  // 表の行を並べて描く。部品の args は使わない
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

function hitAt(x: number, y: number) {
  return document.elementFromPoint(x, y);
}

async function rowsOf(canvasElement: HTMLElement) {
  return waitFor(() => {
    const rows = [...canvasElement.querySelectorAll<HTMLElement>('tr[data-row]')];
    if (rows.length !== ROWS.length) throw new Error('行がまだ出ていない');
    return rows;
  });
}

/**
 * 狭い形（TASK-524）。どの行でも、ID の中心はその行のコピーのボタンに、ID の右の余白と
 * タイトルの下の余白はその行のリンクに当たる（ほかの行・一番下の行へ漏れない）。
 */
export const Narrow: Story = {
  render: renderTable(true),
  play: async ({ canvasElement }) => {
    for (const row of await rowsOf(canvasElement)) {
      row.scrollIntoView({ block: 'center' });
      const copy = row.querySelector<HTMLElement>('[data-task-id-copy]')!;
      const link = row.querySelector('a[data-task-seq-key]')!;
      const copyBox = copy.getBoundingClientRect();
      await expect(
        hitAt(copyBox.left + copyBox.width / 2, copyBox.top + copyBox.height / 2)?.closest(
          '[data-task-id-copy]',
        ),
      ).toBe(copy);
      // ID の右の余白（ID の段の右端の手前）
      const line = copy.parentElement!.getBoundingClientRect();
      await expect(
        hitAt(line.right - 4, line.top + line.height / 2)?.closest('a[data-task-seq-key]'),
      ).toBe(link);
      // タイトルの下の余白（セルの中の div の下端の手前、ID の段より左ではない右側）
      const cell = copy.closest('td')!.firstElementChild!.getBoundingClientRect();
      await expect(hitAt(cell.right - 4, cell.bottom - 2)?.closest('a[data-task-seq-key]')).toBe(
        link,
      );
    }
  },
};

/** 対照: 広い形はセルを的にしない。リンクの右の余白はリンクに当たらない */
export const Wide: Story = {
  render: renderTable(false),
  play: async ({ canvasElement }) => {
    for (const row of await rowsOf(canvasElement)) {
      row.scrollIntoView({ block: 'center' });
      await expect(row.querySelector('[data-task-id-copy]')).toBeNull();
      const cell = row.querySelector('td')!.firstElementChild!.getBoundingClientRect();
      const link = row.querySelector('a[data-task-seq-key]')!;
      const linkBox = link.getBoundingClientRect();
      // リンクの文字より右（セルの右端の手前）は、リンクではない
      await expect(linkBox.right).toBeLessThan(cell.right - 8);
      await expect(
        hitAt(cell.right - 4, linkBox.top + linkBox.height / 2)?.closest('a[data-task-seq-key]'),
      ).toBeNull();
    }
  },
};

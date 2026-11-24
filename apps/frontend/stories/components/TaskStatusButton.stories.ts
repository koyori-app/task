import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import TaskStatusButton from '@/components/tasks/TaskStatusButton.vue';

const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const status = (
  id: string,
  name: string,
  color: string,
  position: number,
  done = false,
  defaultDone = false,
) => ({
  id,
  name,
  color,
  position,
  is_default: position === 0,
  is_done_state: done,
  is_default_done: defaultDone,
  project_id: PROJECT_UUID,
  created_at: '2026-07-15T00:00:00Z',
});

/** 4 段のワークフロー。未完了の 3 つが 点線の円 → 1/3 → 2/3 と埋まり、完了は塗りの円 */
const statuses = [
  status('backlog', 'Backlog', '#94a3b8', 0),
  status('progress', 'In Progress', '#3b82f6', 1),
  status('review', 'In Review', '#a855f7', 2),
  status('done', 'Done', '#22c55e', 3, true, true),
];

const onUpdateStatusId = fn();

const meta = {
  title: 'Components/Tasks/TaskStatusButton',
  component: TaskStatusButton,
  tags: ['autodocs'],
  // emit の受け口は props の型に無いので、spy は外に持って渡す
  args: { statuses, statusId: 'progress', 'onUpdate:statusId': onUpdateStatusId } as never,
  beforeEach: () => {
    onUpdateStatusId.mockClear();
  },
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'タスク詳細のステータス。ステータスの色を 16% に薄めた背景のボタン（押すとメニュー）と、離して置いた ✓（既定の完了へ移す）。アイコンは並び順から計算した「円の埋まり方」。',
      },
    },
  },
} satisfies Meta<typeof TaskStatusButton>;

export default meta;
type Story = StoryObj<typeof meta>;

const fillOf = (canvasElement: HTMLElement) =>
  canvasElement.querySelector('[data-task-status-icon]')!.getAttribute('data-fill');

export const AllStatuses: Story = {
  name: 'ステータス 4 種（円の埋まり方）',
  render: () => ({
    components: { TaskStatusButton },
    setup: () => ({ statuses }),
    template: `
      <div class="flex flex-col gap-3">
        <TaskStatusButton v-for="status in statuses" :key="status.id" :statuses="statuses" :status-id="status.id" />
      </div>
    `,
  }),
  play: async ({ canvasElement }) => {
    const icons = [...canvasElement.querySelectorAll('[data-task-status-icon]')];
    await expect(icons.map((icon) => icon.getAttribute('data-fill'))).toEqual([
      'empty',
      'partial',
      'partial',
      'done',
    ]);
    // 完了済みには ✓ を出さない
    const rows = [...canvasElement.querySelectorAll('[data-task-status-button]')];
    await expect(rows.map((row) => !!row.querySelector('[data-task-status-done]'))).toEqual([
      true,
      true,
      true,
      false,
    ]);
    // 背景はステータスの色を薄めたもの、文字は常に濃い色
    const pill = rows[1].querySelector<HTMLElement>('[data-task-status-name]')!;
    const probe = document.createElement('span');
    probe.className = 'text-foreground';
    canvasElement.append(probe);
    await expect(getComputedStyle(pill).color).toBe(getComputedStyle(probe).color);
    probe.remove();
  },
};

export const Default: Story = {
  name: '進行中（✓ で既定の完了へ）',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(fillOf(canvasElement)).toBe('partial');
    const done = canvas.getByRole('button', { name: 'Done にする（完了）' });
    // ✓ は 32px の枠つきのボタン。メニューのボタンとは離して置く
    const rect = done.getBoundingClientRect();
    await expect([rect.width, rect.height]).toEqual([32, 32]);
    const trigger = canvas.getByRole('button', { name: 'ステータス' });
    await expect(rect.left - trigger.getBoundingClientRect().right).toBeGreaterThanOrEqual(8);
    await userEvent.click(done);
    await expect(onUpdateStatusId).toHaveBeenCalledWith('done');
  },
};

export const HoverDone: Story = {
  name: '✓ のホバー（緑の背景に白の ✓）の規則がある',
  play: async ({ canvasElement }) => {
    const done = within(canvasElement).getByRole('button', { name: 'Done にする（完了）' });
    // 合成のイベントでは :hover が掛からないので、ホバーの class に規則が生成されていることを見る
    // （任意の値の class は、書き間違えると黙って規則が出ない）
    const walk = (rules: CSSRuleList): CSSRule[] =>
      [...rules].flatMap((rule) =>
        'cssRules' in rule ? [rule, ...walk((rule as CSSGroupingRule).cssRules)] : [rule],
      );
    const rules = [...document.styleSheets].flatMap((sheet) => {
      try {
        return walk(sheet.cssRules);
      } catch {
        return [];
      }
    });
    // 入れ子（`.x { &:hover {…} }`）でも、平らにした形（`.x:hover {…}`）でも拾う
    const ruleFor = (className: string) =>
      rules
        .filter(
          (rule) =>
            'selectorText' in rule &&
            (rule as CSSStyleRule).selectorText.startsWith(`.${CSS.escape(className)}`),
        )
        .map((rule) => rule.cssText)
        .join('\n');
    const background = 'hover:bg-[var(--koyori-color-success)]';
    await expect(done).toHaveClass(background, 'hover:text-white');
    await expect(ruleFor(background)).toContain(':hover');
    await expect(ruleFor(background)).toContain('var(--koyori-color-success)');
    await expect(ruleFor('hover:text-white')).toContain(':hover');
  },
};

export const MenuOpen: Story = {
  name: 'メニューを開いた状態（今のものに ✓）',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(canvas.getByRole('button', { name: 'ステータス' }));
    await expect(await page.findByRole('menuitemradio', { name: 'In Progress' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    await userEvent.click(page.getByRole('menuitemradio', { name: 'In Review' }));
    await expect(onUpdateStatusId).toHaveBeenCalledWith('review');
  },
};

export const DoneBlocked: Story = {
  name: '完了条件を満たしていない（✓ は押せず、理由を出す）',
  args: { doneBlocked: true, remainingAssignees: 2 },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const done = canvas.getByRole('button', { name: 'Done にする（完了）' });
    await expect(done).toHaveAttribute('aria-disabled', 'true');
    await userEvent.click(done);
    await expect(onUpdateStatusId).not.toHaveBeenCalled();
    // フォーカスすると理由が見える
    done.focus();
    await userEvent.hover(done);
    await waitFor(() => expect(page.getByText('担当者があと 2 人残っています')).toBeVisible());
    // メニューでも完了扱いは選べない
    await userEvent.click(canvas.getByRole('button', { name: 'ステータス' }));
    await expect(await page.findByRole('menuitemradio', { name: 'Done' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
  },
};

export const LongName: Story = {
  name: '長い名前は「…」で切る',
  args: {
    statuses: [
      status('long', 'お客さまの確認を待っている（返事が来るまで動かせない）', '#f97316', 0),
      ...statuses.slice(1),
    ],
    statusId: 'long',
  },
  play: async ({ canvasElement }) => {
    const name = canvasElement.querySelector<HTMLElement>('[data-task-status-name]')!;
    // 既定の書体のまま測る。1 行に収まり、はみ出した分は切れている
    await expect(name.scrollWidth).toBeGreaterThan(name.clientWidth);
    await expect(name.getBoundingClientRect().height).toBeLessThanOrEqual(24);
    const pill = name.closest<HTMLElement>('.max-w-48')!;
    await expect(pill.getBoundingClientRect().width).toBeLessThanOrEqual(192);
    // ✓ は押し出されずに残る
    await expect(
      within(canvasElement).getByRole('button', { name: 'Done にする（完了）' }),
    ).toBeVisible();
  },
};

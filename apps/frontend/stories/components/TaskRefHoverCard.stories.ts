import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, within, waitFor, fireEvent } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import { createPinia, setActivePinia } from 'pinia';
import CommentBody from '@/components/tasks/CommentBody.vue';
import { htmlTheme } from '../html-theme';
/** 開くまで通信せず、成功は共有キャッシュ、403・404は閲覧不可として表示する。
 * カードは状態をアイコン（円の埋まり方）で出し、説明 2 行とラベルを持つ。開くリンクは持たない。 */
const TENANT = '11111111-1111-4111-8111-111111111111';
const USER = '22222222-2222-4222-8222-222222222222';
type Mode =
  | 'ready'
  | 'minimal'
  | 'overdue'
  | 'loading'
  | '404'
  | '403'
  | 'forbidden'
  | 'failed'
  | 'other';
let taskCalls = 0;
function mock(mode: Mode) {
  return () => {
    setActivePinia(createPinia());
    taskCalls = 0;
    const original = globalThis.fetch;
    globalThis.fetch = async (request, init) => {
      const url = new URL(
        typeof request === 'string' ? request : request instanceof URL ? request.href : request.url,
        'http://localhost',
      );
      if (!url.pathname.includes(`/tenants/${TENANT}/`)) return original(request, init);
      const json = (body: unknown, status = 200) =>
        new Response(JSON.stringify(body), {
          status,
          headers: { 'Content-Type': 'application/json' },
        });
      if (url.pathname.endsWith('/projects'))
        return json(
          mode === 'forbidden'
            ? []
            : [
                {
                  id: 'project',
                  key: mode === 'other' ? 'ENG' : 'TASK',
                  name: mode === 'other' ? '別のプロジェクト' : 'タスク管理',
                },
              ],
        );
      if (url.pathname.endsWith('/statuses'))
        return json([
          { id: 'todo', name: '未着手', color: '#94a3b8', position: 0, is_done_state: false },
          { id: 'status', name: '進行中', color: '#3b82f6', position: 1, is_done_state: false },
          { id: 'done', name: '完了', color: '#22c55e', position: 2, is_done_state: true },
        ]);
      if (url.pathname.includes('/users/')) return json({ username: 'taro', membership: 'Member' });
      taskCalls++;
      if (mode === 'loading') return new Promise<Response>(() => {});
      if (mode === '404' || mode === '403' || mode === 'failed')
        return json({ message: 'failure' }, mode === '404' ? 404 : mode === '403' ? 403 : 500);
      return json({
        seq_id: 58,
        completed_at: null,
        hard_deadline: null,
        title: 'タスクの参照リンクに情報のカードを表示する',
        status_id: mode === 'minimal' ? null : 'status',
        priority: 'High',
        assignees:
          mode === 'minimal'
            ? []
            : Array.from({ length: 4 }, (_, i) => ({
                user: { id: `${i}`, username: `担当者${i}` },
              })),
        soft_deadline:
          mode === 'minimal'
            ? null
            : mode === 'overdue'
              ? '2020-01-01T00:00:00Z'
              : '2099-01-01T00:00:00Z',
        is_archived: mode === 'overdue',
        description:
          mode === 'minimal'
            ? null
            : '説明の中の TASK ID にホバーすると、タスクの情報を出す。\n一覧の行でも同じカードを使う。\n3 行目は切れる。',
        labels: mode === 'minimal' ? [] : [{ id: 'l1', name: 'frontend', color: '#3b82f6' }],
      });
    };
    return () => {
      globalThis.fetch = original;
    };
  };
}
const meta = {
  title: 'Tasks/TaskRefHoverCard',
  parameters: { a11y: { test: 'error' } },
  decorators: [
    () => ({
      setup() {
        provide(
          VUE_QUERY_CLIENT,
          new QueryClient({ defaultOptions: { queries: { retry: false } } }),
        );
      },
      template: '<story />',
    }),
  ],
} satisfies Meta;
export default meta;
type Story = StoryObj<typeof meta>;
function render(light = false, body = 'TASK-58 を見てください。') {
  return () => ({
    components: { CommentBody },
    setup: () => ({ TENANT, body, mentions: { [USER]: { status: 'member', name: 'taro' } } }),
    template: `<div ${light ? 'data-theme="light"' : 'class="dark"'} style="padding:16px;min-height:360px;background:var(--koyori-color-surface);color:var(--koyori-color-text)"><CommentBody :body="body" comment-id="story" :tenant-id="TENANT" mention-tenant="acme" :mentions="mentions" /></div>`,
  });
}
async function open(canvasElement: HTMLElement, key = 'TASK-58') {
  const canvas = within(canvasElement);
  const link = await canvas.findByRole('link', { name: key });
  await userEvent.hover(link);
  const dialog = await canvas.findByRole('dialog', { name: `${key} のタスク` });
  await waitFor(() => expect(dialog).toBeVisible());
  return { canvas, link, dialog };
}
function story(mode: Mode, light = false): Story {
  return {
    // 明るい表示は html の data-theme で定める。本文の色（remark-gfm のリンクなど）は html の
    // data-theme を見るので、内側の箱の属性だけでは暗い表示の色が残る。カードも body へ出る
    beforeEach: light ? [mock(mode), htmlTheme('light')] : mock(mode),
    render: render(light, mode === 'other' ? 'ENG-58 を見てください。' : undefined),
    play: async ({ canvasElement }) => {
      const { canvas, link, dialog } = await open(
        canvasElement,
        mode === 'other' ? 'ENG-58' : undefined,
      );
      const card = within(dialog);
      if (mode === '404' || mode === '403' || mode === 'forbidden') {
        await expect(
          await card.findByText('タスクが見つからないか、閲覧する権限がありません。'),
        ).toBeVisible();
        await expect(card.queryByRole('link')).toBeNull();
        if (mode === 'forbidden') await expect(taskCalls).toBe(0);
        return;
      }
      if (mode === 'loading') {
        await expect(card.getByText('読み込み中…')).toBeVisible();
        await expect(card.getByText('TASK-58')).toBeVisible();
        return;
      }
      if (mode === 'failed') {
        await expect(await card.findByText('タスクの情報を取得できませんでした。')).toBeVisible();
      } else {
        await expect(
          await card.findByText('タスクの参照リンクに情報のカードを表示する'),
        ).toBeVisible();
        await expect(card.getByText('高')).toBeVisible();
        await expect(card.getByText(mode === 'minimal' ? '担当者なし' : '+1')).toBeVisible();
        if (mode === 'overdue') {
          await expect(card.getByText('アーカイブ済み')).toBeVisible();
          const overdue = card.getByText(/日超過/);
          const probe = document.createElement('span');
          probe.style.color = 'var(--koyori-color-danger)';
          dialog.append(probe);
          await expect(getComputedStyle(overdue).color).toBe(getComputedStyle(probe).color);
          probe.remove();
        }
        // ID は出す。プロジェクト名は出さない
        await expect(card.getByText(mode === 'other' ? 'ENG-58' : 'TASK-58')).toBeVisible();
        await expect(card.queryByText(/別のプロジェクト|タスク管理/)).toBeNull();
        if (mode === 'minimal') {
          await expect(dialog.querySelector('[data-task-ref-description]')).toBeNull();
          await expect(card.queryByRole('list', { name: 'ラベル' })).toBeNull();
        } else {
          // ステータスはアイコンで出し、名前は読み上げ専用。進行中は 3 段の 2 番目 = 扇形
          await expect(dialog.querySelector('[data-task-status-icon]')).toHaveAttribute(
            'data-fill',
            'partial',
          );
          await expect(card.getByText('進行中')).toHaveClass('sr-only');
          await expect(card.getByText('frontend')).toBeVisible();
          // 説明は 2 行で切る（3 行ぶんの高さにならない）
          const description = dialog.querySelector<HTMLElement>('[data-task-ref-description]')!;
          const lineHeight = Number.parseFloat(getComputedStyle(description).lineHeight);
          await expect(description.getBoundingClientRect().height).toBeLessThanOrEqual(
            lineHeight * 2 + 1,
          );
          await expect(description.scrollHeight).toBeGreaterThan(description.clientHeight);
        }
      }
      // 幅は 320px。開くリンクと「Esc で閉じる」の行は持たない
      await expect(dialog.getBoundingClientRect().width).toBe(320);
      await expect(getComputedStyle(dialog).borderRadius).toBe('6px');
      await expect(card.queryByRole('link')).toBeNull();
      await expect(card.queryByText(/Esc/)).toBeNull();
      await userEvent.unhover(link);
      await waitFor(() => expect(canvas.queryByRole('dialog')).toBeNull());
      await userEvent.hover(link);
      await canvas.findByRole('dialog', { name: /のタスク/ });
      await waitFor(() => expect(taskCalls).toBe(mode === 'failed' ? 2 : 1));
    },
  };
}
export const Ready = story('ready');
export const ReadyLight = story('ready', true);
export const Minimal = story('minimal');
export const OverdueArchived = story('overdue');
export const Loading = story('loading');
export const LoadingLight = story('loading', true);
export const NotFound = story('404');
export const ForbiddenResponse = story('403');
export const Forbidden = story('forbidden');
export const Failed = story('failed');
export const OtherProject = story('other');
export const WithMention: Story = {
  beforeEach: mock('ready'),
  render: render(false, `<@${USER}> TASK-58`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const chip = await canvas.findByRole('link', { name: '@taro' });
    await userEvent.hover(chip);
    await canvas.findByRole('dialog', { name: 'taro のプロフィール' });
    await expect(canvas.queryByRole('dialog', { name: 'TASK-58 のタスク' })).toBeNull();
    await userEvent.unhover(chip);
    await waitFor(() => expect(canvas.queryByRole('dialog')).toBeNull());
    await open(canvasElement);
    await expect(canvas.queryByRole('dialog', { name: 'taro のプロフィール' })).toBeNull();
  },
};
export const Keyboard: Story = {
  beforeEach: mock('ready'),
  render: render(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const link = await canvas.findByRole('link', { name: 'TASK-58' });
    await userEvent.tab();
    await expect(link).toHaveFocus();
    await canvas.findByRole('dialog', { name: 'TASK-58 のタスク' });
    // カードの中に操作は無い（開くのは起点のリンク）。Esc で閉じる
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(canvas.queryByRole('dialog')).toBeNull());
  },
};
export const Touch: Story = {
  beforeEach: mock('ready'),
  render: render(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const link = await canvas.findByRole('link', { name: 'TASK-58' });
    await fireEvent.pointerOver(link, { pointerType: 'touch' });
    await new Promise((resolve) => setTimeout(resolve, 700));
    await expect(canvas.queryByRole('dialog')).toBeNull();
    await expect(taskCalls).toBe(0);
  },
};
export const InCode: Story = {
  beforeEach: mock('ready'),
  render: render(false, '`TASK-58`'),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('TASK-58');
    await expect(canvas.queryByRole('link')).toBeNull();
    await expect(canvas.queryByRole('dialog')).toBeNull();
  },
};

import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import type { components } from '@/generated/api';
import TaskRow from '../TaskRow.vue';

enableAutoUnmount(afterEach);

type Task = components['schemas']['TaskResponse'];
type Status = components['schemas']['ProjectStatusResponse'];

const statuses = [
  { id: 'todo', name: 'Todo', color: '#888888', position: 0, is_done_state: false },
  { id: 'doing', name: 'Doing', color: '#0000ff', position: 1, is_done_state: false },
  { id: 'done', name: 'Done', color: '#00aa00', position: 2, is_done_state: true },
] as Status[];

const user = (name: string) => ({ id: name, username: name, avatar_url: null });

function task(overrides: Partial<Task> = {}): Task {
  return {
    id: 'task-1',
    project_id: 'project',
    seq_id: 42,
    title: 'ログイン画面の修正',
    status_id: 'doing',
    priority: 'High',
    progress_pct: 0,
    is_archived: false,
    assignees: [{ role: 'assignee', user: user('yupix') }],
    labels: [
      { id: 'l1', name: 'bug', color: '#ff0000' },
      { id: 'l2', name: 'frontend', color: '#00ff00' },
    ],
    soft_deadline: '2020-01-10T00:00:00.000Z',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  } as Task;
}

function mountRow(props: Record<string, unknown> = {}) {
  return mount(TaskRow, {
    props: { task: task(), projectKey: 'ENG', tenantDisplayId: 'acme', statuses, ...props },
    attachTo: document.body,
  });
}

const statusTrigger = (wrapper: ReturnType<typeof mountRow>) =>
  wrapper.get('button[aria-label^="ENG-42 のステータス"]');
const menuItem = (wrapper: ReturnType<typeof mountRow>, name: string) =>
  wrapper.findAll('[role="menuitemradio"]').find((item) => item.text() === name)!;

describe('TaskRow', () => {
  it('ステータスのアイコン・題・ID・ラベル・担当・期限・優先度を出す', () => {
    const wrapper = mountRow();

    expect(wrapper.get('[data-task-status-icon]').attributes('data-fill')).toBe('partial');
    const link = wrapper.get('a');
    expect(link.text()).toBe('ログイン画面の修正');
    expect(link.attributes('href')).toBe('/acme/projects/ENG/tasks/ENG-42');
    expect(link.classes()).toContain('font-medium');
    expect(wrapper.get('[data-task-row-card] .font-mono').text()).toBe('ENG-42');
    expect(wrapper.findAll('[data-task-row-label]').map((el) => el.text())).toEqual([
      'bug',
      'frontend',
    ]);
    expect(wrapper.get('[data-task-row-assignees]').attributes('aria-label')).toBe('担当者: yupix');
    // 過ぎた期限は赤
    const deadline = wrapper.get('[data-task-row-deadline]');
    expect(deadline.text()).toContain('超過');
    expect(deadline.classes()).toContain('text-destructive');
    expect(wrapper.get('[data-task-row-priority]').text()).toContain('高');
  });

  it('ステータスの名前は文字で並べず、ホバーと読み上げ名に出す', () => {
    const wrapper = mountRow();

    expect(statusTrigger(wrapper).attributes('aria-label')).toBe('ENG-42 のステータス: Doing');
    expect(statusTrigger(wrapper).element.closest('[title]')!.getAttribute('title')).toBe('Doing');
    // 行の見える所にステータス名は無い（メニューの項目を除く）
    const visible = wrapper.get('[data-task-row-card]').text();
    expect(visible).not.toContain('Doing');
  });

  it('担当が 3 人を超えたら +N、期限が無ければ期限の列は空', () => {
    const wrapper = mountRow({
      task: task({
        assignees: ['a', 'b', 'c', 'd'].map((name) => ({ role: 'assignee', user: user(name) })),
        soft_deadline: null,
      }),
    });

    expect(wrapper.get('[data-task-row-assignees]').text()).toContain('+1');
    expect(wrapper.get('[data-task-row-deadline]').text()).toBe('');
    const none = mountRow({ task: task({ assignees: [] }) });
    expect(none.get('[data-task-row-assignees]').attributes('aria-label')).toBe('担当者なし');
  });

  it('完了したタスクは期限を赤くしない', () => {
    const wrapper = mountRow({ task: task({ status_id: 'done' }) });
    expect(wrapper.get('[data-task-row-deadline]').classes()).not.toContain('text-destructive');
    expect(wrapper.get('[data-task-status-icon]').attributes('data-fill')).toBe('done');
  });

  it('題のリンクは open を、メニューは update:status を emit する', async () => {
    const wrapper = mountRow();

    await wrapper.get('a').trigger('click');
    expect(wrapper.emitted('open')).toEqual([[wrapper.props('task')]]);

    await menuItem(wrapper, 'Done').trigger('click');
    expect(wrapper.emitted('update:status')).toEqual([[wrapper.props('task'), 'done']]);
  });

  it('今のステータスを選び直しても emit しない', async () => {
    const wrapper = mountRow();
    await menuItem(wrapper, 'Doing').trigger('click');
    expect(wrapper.emitted('update:status')).toBeUndefined();
  });

  it('pending の間はメニューが止まる', async () => {
    const wrapper = mountRow({ pending: true });

    expect(statusTrigger(wrapper).attributes('disabled')).toBeDefined();
    await menuItem(wrapper, 'Done').trigger('click');
    expect(wrapper.emitted('update:status')).toBeUndefined();
  });

  it('失敗は行の下に出す', () => {
    expect(mountRow().find('[role="alert"]').exists()).toBe(false);
    const wrapper = mountRow({ error: '更新に失敗しました' });
    expect(wrapper.get('[role="alert"]').text()).toBe('更新に失敗しました');
  });

  it('ホバーカードの起点は題と ID のブロックだけ（ステータスのメニューと右の列は入れない）', () => {
    const wrapper = mountRow();

    const card = wrapper.get('[data-task-row-card]');
    expect(card.attributes('data-task-id')).toBe('task-1');
    expect(card.find('a').exists()).toBe(true);
    expect(card.find('button').exists()).toBe(false);
    expect(card.find('[data-task-row-assignees]').exists()).toBe(false);
    expect(card.find('[data-task-row-priority]').exists()).toBe(false);
  });

  it('狭い器では期限と優先度を隠し、担当は残す', () => {
    const wrapper = mountRow();

    for (const selector of ['[data-task-row-deadline]', '[data-task-row-priority]']) {
      expect(wrapper.get(selector).classes()).toEqual(
        expect.arrayContaining(['hidden', '@md:flex']),
      );
    }
    expect(wrapper.get('[data-task-row-assignees]').classes()).not.toContain('hidden');
  });

  it('枠と内側の余白を持たず、ホバーの背景だけを左右へはみ出させる', () => {
    const inner = mountRow().get('[data-task-row] > div');
    expect(inner.classes()).toEqual(expect.arrayContaining(['-mx-2', 'px-2', 'rounded-md']));
    expect(inner.classes().some((name) => name.startsWith('border'))).toBe(false);
  });
});

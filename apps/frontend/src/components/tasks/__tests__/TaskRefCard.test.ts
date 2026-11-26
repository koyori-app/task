import { it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TaskRefCard from '../TaskRefCard.vue';
const base = {
  state: 'ready' as const,
  seqKey: 'TASK-58',
  href: '/acme/projects/TASK/tasks/TASK-58',
};
it('タスクの属性を表示する。状態はアイコンで出し、名前は読み上げにだけ渡す', () => {
  const wrapper = mount(TaskRefCard, {
    props: {
      ...base,
      title: '題',
      description: '1 行目\n2 行目\n3 行目',
      status: {
        name: '進行中',
        color: '#fff',
        done: false,
        fill: { kind: 'partial', fraction: 0.5 },
      },
      priority: 'High',
      deadline: { label: '今日', overdue: true },
      labels: [{ id: 'l1', name: 'bug', color: '#f00' }],
      archived: true,
    },
  });
  for (const text of [
    'TASK-58',
    '題',
    '1 行目',
    '高',
    '今日',
    'bug',
    'アーカイブ済み',
    '担当者なし',
  ])
    expect(wrapper.text()).toContain(text);
  // ステータスの名前は見える文字にしない
  expect(wrapper.get('.sr-only').text()).toBe('進行中');
  expect(wrapper.get('[data-task-status-icon]').attributes('data-fill')).toBe('partial');
  // 説明は 2 行で切る（CSS の line-clamp。中身は全部渡す）
  expect(wrapper.get('[data-task-ref-description]').classes()).toContain('description');
  // プロジェクト名と「タスクを開く / Esc で閉じる」の行は出さない
  expect(wrapper.find('a').exists()).toBe(false);
  expect(wrapper.text()).not.toContain('Esc');
});
it('絵文字を持つラベルは色の丸の代わりに絵文字を出す', () => {
  const wrapper = mount(TaskRefCard, {
    props: {
      ...base,
      title: '題',
      labels: [
        { id: 'l1', name: 'デザイン', color: '#8b5cf6', icon_emoji: '🎨' },
        { id: 'l2', name: 'bug', color: '#f00', icon_emoji: null },
      ],
    },
  });
  const [design, bug] = wrapper.findAll('.label');
  expect(design!.text()).toBe('🎨デザイン');
  expect(design!.find('[style*="background-color"]').exists()).toBe(false);
  // 対照: 絵文字を持たないラベルは色の丸
  expect(bug!.find('[style*="background-color"]').exists()).toBe(true);
});
it('説明もラベルも無いタスクは、その行を出さない', () => {
  const wrapper = mount(TaskRefCard, { props: { ...base, title: '題', labels: [] } });
  expect(wrapper.find('[data-task-ref-description]').exists()).toBe(false);
  expect(wrapper.find('.labels').exists()).toBe(false);
});
it.each([0, 3, 4])('担当者 %i 人の境界', (count) => {
  const wrapper = mount(TaskRefCard, {
    props: {
      ...base,
      assignees: Array.from({ length: count }, (_, i) => ({ id: `${i}`, username: `user${i}` })),
    },
  });
  expect(wrapper.findAllComponents({ name: 'UserAvatar' })).toHaveLength(Math.min(count, 3));
  expect(wrapper.text().includes('+1')).toBe(count === 4);
});
it.each(['loading', 'unavailable', 'error'] as const)('%s の文言', (state) => {
  const wrapper = mount(TaskRefCard, { props: { ...base, state } });
  expect(wrapper.text()).toContain(
    {
      loading: '読み込み中…',
      unavailable: 'タスクが見つからないか、閲覧する権限がありません。',
      error: 'タスクの情報を取得できませんでした。',
    }[state],
  );
  expect(wrapper.find('.attribute').exists()).toBe(false);
});

it('担当者の名前をグループの読み上げに出す', () => {
  const wrapper = mount(TaskRefCard, {
    props: { ...base, assignees: [{ id: '1', username: 'alice' }] },
  });
  expect(wrapper.get('[role="group"]').attributes('aria-label')).toBe('担当者: alice');
});
it.each(['loading', 'unavailable', 'error'] as const)(
  '%s で古いアーカイブの印を出さない',
  (state) => {
    const wrapper = mount(TaskRefCard, { props: { ...base, state, archived: true } });
    expect(wrapper.text()).not.toContain('アーカイブ済み');
  },
);

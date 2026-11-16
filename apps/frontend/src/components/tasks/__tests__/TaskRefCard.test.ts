import { it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TaskRefCard from '../TaskRefCard.vue';
const base = {
  state: 'ready' as const,
  seqKey: 'TASK-58',
  href: '/acme/projects/TASK/tasks/TASK-58',
};
it('タスクの属性とリンクを表示する', () => {
  const wrapper = mount(TaskRefCard, {
    props: {
      ...base,
      title: '題',
      status: { name: '進行中', color: '#fff', done: false },
      priority: 'High',
      deadline: { label: '今日', overdue: true },
      archived: true,
    },
  });
  for (const text of ['題', '進行中', '高', '今日', 'アーカイブ済み', '担当者なし'])
    expect(wrapper.text()).toContain(text);
  expect(wrapper.get('a.kfm-task-ref').attributes('href')).toBe(base.href);
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
it.each(['loading', 'unavailable', 'error'] as const)('%s の文言とリンク', (state) => {
  const wrapper = mount(TaskRefCard, { props: { ...base, state } });
  expect(wrapper.find('a').exists()).toBe(state !== 'unavailable');
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

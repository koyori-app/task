import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount, type VueWrapper } from '@vue/test-utils';

import TaskListSortHeader from '@/components/tasks/TaskListSortHeader.vue';
import { TASK_LIST_SORT_COLUMNS } from '@/components/tasks/task-list-sort';

enableAutoUnmount(afterEach);

function mountHeader(sorting: { id: string; desc: boolean }[] = []) {
  return mount(TaskListSortHeader, {
    props: {
      column: TASK_LIST_SORT_COLUMNS[0],
      sorting,
    },
  });
}

/** 項目は閉じていても DOM にあるので、名前で引く */
function menuItem(wrapper: VueWrapper, name: string) {
  const item = wrapper.findAll('[role^="menuitem"]').find((el) => el.text() === name);
  if (!item) throw new Error(`menu item ${name} not found`);
  return item;
}

describe('TaskListSortHeader', () => {
  it('昇順・降順を選ぶと単一列の並びを通知する', async () => {
    const wrapper = mountHeader();

    await menuItem(wrapper, '名前の昇順').trigger('click');
    await menuItem(wrapper, '名前の降順').trigger('click');

    expect(wrapper.emitted('update:sorting')).toEqual([
      [[{ id: 'title', desc: false }]],
      [[{ id: 'title', desc: true }]],
    ]);
  });

  it('並び順は radio で、選ぶとメニューが閉じる', async () => {
    const wrapper = mountHeader();
    const trigger = wrapper.get('button[aria-haspopup="menu"]');
    const menu = wrapper.get('[role="menu"]');

    await trigger.trigger('click');
    expect(menu.attributes('hidden')).toBeUndefined();

    const asc = menuItem(wrapper, '名前の昇順');
    expect(asc.attributes('role')).toBe('menuitemradio');
    await asc.trigger('click');

    expect(menu.attributes('hidden')).toBeDefined();
    expect(trigger.attributes('aria-expanded')).toBe('false');
  });

  it('有効な並びをトリガーに示し、クリアできる', async () => {
    const wrapper = mountHeader([{ id: 'title', desc: true }]);

    expect(wrapper.get('button[aria-haspopup="menu"]').attributes('aria-label')).toBe(
      'タスクを並べ替え、現在は名前の降順',
    );
    expect(menuItem(wrapper, '名前の降順').attributes('aria-checked')).toBe('true');
    const clear = menuItem(wrapper, '並べ替えをクリア');
    expect(clear.attributes('aria-disabled')).toBeUndefined();
    await clear.trigger('click');

    expect(wrapper.emitted('update:sorting')).toEqual([[[]]]);
  });

  it.each([
    [false, 'タスクを名前の降順に反転', true],
    [true, 'タスクを名前の昇順に反転', false],
  ])('表示中の矢印を押すと並びを反転する', async (desc, label, expectedDesc) => {
    const wrapper = mountHeader([{ id: 'title', desc }]);

    await wrapper.get(`button[aria-label="${label}"]`).trigger('click');

    expect(wrapper.emitted('update:sorting')).toEqual([[[{ id: 'title', desc: expectedDesc }]]]);
  });

  it('別の列が有効なときはクリア操作を無効にする', async () => {
    const wrapper = mountHeader([{ id: 'priority', desc: false }]);

    expect(wrapper.get('button[aria-haspopup="menu"]').attributes('aria-label')).toBe(
      'タスクを並べ替え',
    );
    const clear = menuItem(wrapper, '並べ替えをクリア');
    expect(clear.attributes('aria-disabled')).toBe('true');
    await clear.trigger('click');
    expect(wrapper.emitted('update:sorting')).toBeUndefined();
  });
});

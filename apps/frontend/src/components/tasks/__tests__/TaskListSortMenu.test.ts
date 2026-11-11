import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { Dropdown } from '@koyori-app/ui-vue';

import TaskListSortMenu from '../TaskListSortMenu.vue';

enableAutoUnmount(afterEach);

function onSelectOf(sorting: { id: string; desc: boolean }[] = []) {
  const wrapper = mount(TaskListSortMenu, { props: { sorting } });
  const dropdown = wrapper.getComponent(Dropdown);
  return {
    wrapper,
    sections: dropdown.props('items') as {
      items: { value: string; checked?: boolean; disabled?: boolean }[];
    }[],
    select: dropdown.props('onSelect') as (value: string) => void,
  };
}

describe('TaskListSortMenu', () => {
  it('列と向きを選ぶと並びを通知する', () => {
    const { wrapper, select } = onSelectOf();

    select('due_date:desc');

    expect(wrapper.emitted('update:sorting')?.[0]).toEqual([[{ id: 'due_date', desc: true }]]);
  });

  it('並べ替えをクリアすると空の並びを通知する', () => {
    const { wrapper, select } = onSelectOf([{ id: 'priority', desc: false }]);

    select('clear');

    expect(wrapper.emitted('update:sorting')?.[0]).toEqual([[]]);
  });

  it('今の並びに印を付け、並びが無いときはクリアを止める', () => {
    const active = onSelectOf([{ id: 'priority', desc: false }]).sections.flatMap((s) => s.items);
    expect(active.find((item) => item.checked)?.value).toBe('priority:asc');

    const none = onSelectOf().sections.flatMap((s) => s.items);
    expect(none.some((item) => item.checked)).toBe(false);
    expect(none.find((item) => item.value === 'clear')?.disabled).toBe(true);
  });
});

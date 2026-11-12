import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import TaskChecklistProgress from '@/components/tasks/TaskChecklistProgress.vue';

enableAutoUnmount(afterEach);

describe('TaskChecklistProgress', () => {
  it('完了数 / 総数と読み上げ文を出す', () => {
    const wrapper = mount(TaskChecklistProgress, { props: { done: 2, total: 5 } });

    expect(wrapper.text()).toContain('2 / 5');
    expect(wrapper.get('.sr-only').text()).toBe('チェックリスト 5 件中 2 件完了');
    expect(wrapper.attributes('title')).toBe('チェックリスト 5 件中 2 件完了');
  });

  it('0 件完了では進んだ分の円を描かない', () => {
    const wrapper = mount(TaskChecklistProgress, { props: { done: 0, total: 5 } });

    expect(wrapper.findAll('circle')).toHaveLength(1);
  });

  it('全部完了では円が満ちる', () => {
    const wrapper = mount(TaskChecklistProgress, { props: { done: 5, total: 5 } });
    const circles = wrapper.findAll('circle');

    expect(circles).toHaveLength(2);
    expect(Number(circles[1]!.attributes('stroke-dashoffset'))).toBeCloseTo(0);
  });
});
